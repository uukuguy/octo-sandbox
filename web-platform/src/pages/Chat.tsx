import { useEffect, useRef, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useAtom, useAtomValue } from 'jotai';
import { messagesAtom, inputAtom, isStreamingAtom, isConnectedAtom } from '../atoms/chat';
import { accessTokenAtom } from '../atoms/auth';
import { wsManager } from '../ws/manager';
import { WsMessage, ChatMessage } from '../api/types';
import { MessageList } from '../components/chat/MessageList';
import { ChatInput } from '../components/chat/ChatInput';

export function ChatPage() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const navigate = useNavigate();
  const [messages, setMessages] = useAtom(messagesAtom);
  const [input, setInput] = useAtom(inputAtom);
  const [isStreaming, setIsStreaming] = useAtom(isStreamingAtom);
  const [isConnected, setIsConnected] = useAtom(isConnectedAtom);
  const accessToken = useAtomValue(accessTokenAtom);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const currentAssistantMessageId = useRef<string | null>(null);

  // Handle incoming WebSocket messages
  const handleWsMessage = useCallback((wsMsg: WsMessage) => {
    switch (wsMsg.type) {
      case 'message': {
        // Full message received (non-streaming)
        if (wsMsg.content) {
          const newMessage: ChatMessage = {
            id: crypto.randomUUID(),
            role: 'assistant',
            content: wsMsg.content,
            created_at: new Date().toISOString(),
          };
          setMessages((prev) => [...prev, newMessage]);
        }
        setIsStreaming(false);
        break;
      }
      case 'streaming_start': {
        setIsStreaming(true);
        currentAssistantMessageId.current = crypto.randomUUID();
        break;
      }
      case 'streaming_chunk': {
        // Streaming chunk received - append to current message
        const msgId = currentAssistantMessageId.current;
        if (msgId) {
          setMessages((prev) => {
            const existing = prev.find((m) => m.id === msgId);
            if (existing) {
              return prev.map((m) =>
                m.id === msgId
                  ? { ...m, content: m.content + (wsMsg.delta || '') }
                  : m
              );
            } else {
              // First chunk - create new assistant message
              return [
                ...prev,
                {
                  id: msgId,
                  role: 'assistant' as const,
                  content: wsMsg.delta || '',
                  created_at: new Date().toISOString(),
                },
              ];
            }
          });
        }
        break;
      }
      case 'streaming_end': {
        setIsStreaming(false);
        currentAssistantMessageId.current = null;
        break;
      }
      case 'error': {
        console.error('WebSocket error:', wsMsg.message);
        setIsStreaming(false);
        break;
      }
    }
  }, [setMessages, setIsStreaming]);

  // Connect to WebSocket on mount
  useEffect(() => {
    if (!sessionId) {
      navigate('/dashboard');
      return;
    }

    if (!accessToken) {
      navigate('/login');
      return;
    }

    // =============================================================================
    // NOTE: Session history loading is not implemented yet.
    // This requires a backend API endpoint like GET /api/sessions/:id/messages
    // When implemented, fetch messages here and setMessages with the history.
    // =============================================================================

    // Set up message handlers
    wsManager.onMessage(handleWsMessage);
    wsManager.onConnect(() => setIsConnected(true));
    wsManager.onDisconnect(() => setIsConnected(false));

    // Connect
    wsManager.connect(sessionId, accessToken);

    // Cleanup on unmount
    return () => {
      wsManager.disconnect();
      setIsConnected(false);
      setIsStreaming(false);
    };
  }, [sessionId, accessToken, handleWsMessage, navigate, setIsConnected, setIsStreaming]);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSend = () => {
    if (!input.trim() || isStreaming) return;

    // Add user message to list
    const userMessage: ChatMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content: input.trim(),
      created_at: new Date().toISOString(),
    };
    setMessages((prev) => [...prev, userMessage]);

    // Send via WebSocket
    wsManager.send(input.trim());

    // Clear input
    setInput('');
  };

  const handleStop = () => {
    // Send cancel request to server via WebSocket
    wsManager.sendCancel();

    // Update local streaming state
    setIsStreaming(false);
    currentAssistantMessageId.current = null;
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-auto mb-4">
        <MessageList messages={messages} />
        <div ref={messagesEndRef} />
      </div>
      <ChatInput
        value={input}
        onChange={setInput}
        onSend={handleSend}
        onStop={handleStop}
        disabled={!isConnected}
        isStreaming={isStreaming}
      />
    </div>
  );
}

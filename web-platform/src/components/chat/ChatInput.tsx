import { useRef, useEffect } from 'react';
import { Send, Square } from 'lucide-react';

interface ChatInputProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  onStop?: () => void;
  disabled?: boolean;
  isStreaming?: boolean;
}

export function ChatInput({
  value,
  onChange,
  onSend,
  onStop,
  disabled,
  isStreaming,
}: ChatInputProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 150)}px`;
    }
  }, [value]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      onSend();
    }
  };

  return (
    <div className="flex items-end gap-2 p-4 bg-white border-t">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Type a message..."
        className="flex-1 resize-none border rounded-lg p-3 max-h-[150px]"
        disabled={disabled}
        rows={1}
      />
      {isStreaming ? (
        <button
          onClick={onStop}
          className="p-3 bg-red-500 text-white rounded-lg hover:bg-red-600"
        >
          <Square className="w-5 h-5" />
        </button>
      ) : (
        <button
          onClick={onSend}
          disabled={disabled || !value.trim()}
          className="p-3 bg-primary text-white rounded-lg hover:opacity-90 disabled:opacity-50"
        >
          <Send className="w-5 h-5" />
        </button>
      )}
    </div>
  );
}

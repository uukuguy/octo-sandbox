import { useMemo } from 'react';

interface TimelineEvent {
  id: string;
  timestamp: number;
  type: 'start' | 'tool_call' | 'tool_result' | 'end' | 'error';
  toolName?: string;
  duration?: number;
  data?: unknown;
}

interface TimelineViewProps {
  events: TimelineEvent[];
}

export function TimelineView({ events }: TimelineViewProps) {
  const sorted = useMemo(() =>
    [...events].sort((a, b) => a.timestamp - b.timestamp),
    [events]
  );

  return (
    <div className="timeline-view">
      {sorted.map((event, idx) => (
        <div key={event.id || idx} className={`timeline-event timeline-${event.type}`}>
          <div className="timeline-marker" />
          <div className="timeline-content">
            <span className="timeline-time">
              {new Date(event.timestamp).toLocaleTimeString()}
            </span>
            <span className="timeline-label">
              {event.type === 'tool_call' && event.toolName}
              {event.type === 'tool_result' && `← ${event.toolName}`}
              {event.type === 'start' && '开始'}
              {event.type === 'end' && '结束'}
              {event.type === 'error' && '错误'}
            </span>
            {event.duration && (
              <span className="timeline-duration">{event.duration}ms</span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

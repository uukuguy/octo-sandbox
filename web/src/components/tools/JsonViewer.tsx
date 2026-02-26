import { useState } from 'react';

interface JsonViewerProps {
  data: unknown;
  name?: string;
}

function JsonValue({ value, depth = 0 }: { value: unknown; depth?: number }) {
  const [expanded, setExpanded] = useState(depth < 2);

  if (value === null) return <span className="json-null">null</span>;
  if (typeof value === 'boolean') return <span className="json-boolean">{value.toString()}</span>;
  if (typeof value === 'number') return <span className="json-number">{value}</span>;
  if (typeof value === 'string') return <span className="json-string">"{value}"</span>;

  if (Array.isArray(value)) {
    if (value.length === 0) return <span>[]</span>;
    return (
      <span>
        <button onClick={() => setExpanded(!expanded)} className="hover:underline cursor-pointer bg-transparent border-none text-inherit">
          [{expanded ? '▼' : '▶'} {value.length} items]
        </button>
        {expanded && (
          <div className="ml-4 border-l border-border">
            {value.map((item, i) => (
              <div key={i}>
                <span className="json-number">{i}: </span>
                <JsonValue value={item} depth={depth + 1} />
              </div>
            ))}
          </div>
        )}
      </span>
    );
  }

  if (typeof value === 'object') {
    const entries = Object.entries(value);
    if (entries.length === 0) return <span>{'{}'}</span>;
    return (
      <span>
        <button onClick={() => setExpanded(!expanded)} className="hover:underline cursor-pointer bg-transparent border-none text-inherit">
          {'{'}{expanded ? '▼' : '▶'} {entries.length} keys{'}'}
        </button>
        {expanded && (
          <div className="ml-4 border-l border-border">
            {entries.map(([k, v]) => (
              <div key={k}>
                <span className="json-key">"{k}"</span>: <JsonValue value={v} depth={depth + 1} />
              </div>
            ))}
          </div>
        )}
      </span>
    );
  }

  return <span>{String(value)}</span>;
}

export function JsonViewer({ data, name }: JsonViewerProps) {
  return (
    <div className="json-viewer">
      {name && <div className="json-key mb-1">"{name}": </div>}
      <JsonValue value={data} />
    </div>
  );
}

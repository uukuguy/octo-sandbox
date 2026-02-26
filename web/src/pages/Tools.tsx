import { ExecutionList } from "@/components/tools/ExecutionList";

export default function Tools() {
  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="px-4 py-2 border-b border-border">
        <h2 className="text-sm font-medium">Tool Executions</h2>
      </div>
      <ExecutionList />
    </div>
  );
}

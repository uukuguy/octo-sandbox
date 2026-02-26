import { TokenBudgetBar } from "@/components/debug/TokenBudgetBar";

export default function Debug() {
  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      <div className="px-4 py-2 border-b border-border">
        <h2 className="text-sm font-medium">Debug Dashboard</h2>
      </div>
      <TokenBudgetBar />
    </div>
  );
}

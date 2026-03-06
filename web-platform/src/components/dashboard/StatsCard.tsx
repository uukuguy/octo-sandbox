interface StatsCardProps {
  title: string;
  value: number;
  icon: React.ReactNode;
}

export function StatsCard({ title, value, icon }: StatsCardProps) {
  return (
    <div className="bg-white p-4 rounded-xl border">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm text-gray-500">{title}</p>
          <p className="text-2xl font-bold mt-1">{value}</p>
        </div>
        <div className="text-primary">{icon}</div>
      </div>
    </div>
  );
}

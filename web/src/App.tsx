import { useAtom } from "jotai";
import { AppLayout } from "./components/layout/AppLayout";
import { activeTabAtom } from "./atoms/ui";
import Chat from "./pages/Chat";
import Tools from "./pages/Tools";
import Debug from "./pages/Debug";

export default function App() {
  const [activeTab] = useAtom(activeTabAtom);

  return (
    <AppLayout>
      {activeTab === "chat" && <Chat />}
      {activeTab === "tools" && <Tools />}
      {activeTab === "debug" && <Debug />}
    </AppLayout>
  );
}

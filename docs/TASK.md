# 🦑 Octo-Sandbox

本项目是一个将主流顶级自主智能体圈养在沙箱中的智能体技能工具模块开发调试环境，旨在为智能体技能的开发和调试提供一个安全、可控的环境。

./github.com/ 目录下是一些相关的参考项目。其中，nanoclaw 是整体设计最接近的，只不过它的目标是以最简、安全、原生Claw Code(Best harness, best model)的方式仿造OpenClaw，octo-sandbox则是企业级自主智能体的工具模块（Skills/MCP/CLI）安全沙箱调试环境。

OpenClaw 基于 pi-mono 为核心智能体，pi_agent_rust 是 pi-mono 的rust改写版，已具备完整的功能。

craft-agents-oss 的 UI 设计实现非常专业，可作为 octo-sandbox UI 的起点

happyclaw 有一些企业级的功能设计实现，可以参考引入

zeroclaw 是一个类 OpenClaw 的 rust 实现



以上提及的所有项目源码都已下载至本项目 ./github.com/ 目录下，可以通过网络搜索（Tavily/SeqrpAPI优先）和Context7 以及  DeepWiki 了解相关项目以及自主智能体工具（Skills/MCP/CLI) 的详细准确信息。



初步设想是构建企业级自主智能体的工具模块（Skills/MCP/CLI）安全沙箱调试环境，用rust构建软件主体智能体，调度沙箱中的ClawCode/OpenClaw，企业级UI为用户主界面，LLM后端支持最强的Anthropic，最通用的 OpenAI，前三候选的 Google Gemini 即可，外部渠道(channels)支持Telegram/Slack/飞书/企业微信即可，能近似ClaudeCode/Openclaw主要功能（用于测试）以及增强的工具模块（Skills/MCP/CLI）安全沙箱调试能力。

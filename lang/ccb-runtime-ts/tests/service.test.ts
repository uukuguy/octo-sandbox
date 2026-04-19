/**
 * Smoke tests for CcbRuntimeService — no network, no subprocess.
 */

import { describe, expect, test } from "bun:test";
import { CcbRuntimeService } from "../src/service.js";
import { ChunkType } from "../src/proto/types.js";

describe("CcbRuntimeService", () => {
  test("initialize returns sessionId and runtimeId", () => {
    const svc = new CcbRuntimeService();
    const resp = svc.initialize({ payload: { sessionId: "test-sid" } });
    expect(resp.sessionId).toBe("test-sid");
    expect(resp.runtimeId).toBe("eaasp-ccb-runtime");
  });

  test("initialize without sessionId generates one", () => {
    const svc = new CcbRuntimeService();
    const resp = svc.initialize({});
    expect(resp.sessionId).toBeTruthy();
  });

  test("getCapabilities tier is aligned", () => {
    const svc = new CcbRuntimeService();
    const cap = svc.getCapabilities();
    expect(cap.tier).toBe("aligned");
  });

  test("health returns healthy=true", () => {
    const svc = new CcbRuntimeService();
    expect(svc.health().healthy).toBe(true);
  });

  test("send yields done chunk after init", async () => {
    // ADR-V2-021: chunkType is the proto ChunkType enum (numeric on wire).
    const svc = new CcbRuntimeService();
    svc.initialize({ payload: { sessionId: "s1" } });
    const chunks: ChunkType[] = [];
    for await (const c of svc.send({ sessionId: "s1", message: { content: "hello" } })) {
      chunks.push(c.chunkType);
    }
    expect(chunks).toContain(ChunkType.DONE);
  });

  test("send without session yields error chunk", async () => {
    const svc = new CcbRuntimeService();
    const chunks = [];
    for await (const c of svc.send({ sessionId: "none", message: { content: "hi" } })) {
      chunks.push(c);
    }
    expect(chunks[0]?.isError).toBe(true);
  });

  test("connectMcp returns connected names", () => {
    const svc = new CcbRuntimeService();
    const resp = svc.connectMcp({
      sessionId: "s1",
      servers: [{ name: "memory", transport: "stdio" }],
    });
    expect(resp.connected).toContain("memory");
  });

  test("terminate clears session", () => {
    const svc = new CcbRuntimeService();
    svc.initialize({ payload: { sessionId: "s1" } });
    svc.terminate();
    expect(svc.getState().sessionId).toBe("");
  });
});

import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ChatMessage,
  ChatMessageType,
  createMessage,
  ExecutionStepMetadata,
  PhasePlanMetadata,
  ClarificationMetadata,
  TakeoverMetadata,
  CompletionMetadata,
} from "../chatTypes";

// Tauri's listen() returns a Promise<UnlistenFn>. The usual cleanup
// `unlisten.then((fn) => fn())` is async — if a useEffect re-runs before
// the unsubscribe resolves, a new listener is attached before the old one
// detaches, and a single Tauri event fires the callback twice. Visible
// symptoms: duplicate chat bubbles, two planning spinners, duplicated
// step entries. This helper sets a synchronous cancelled flag that the
// callback checks (and unsubscribes the moment the promise resolves if
// cleanup has already happened), eliminating the race.
function useSafeTauriListener<T = any>(
  event: string,
  handler: (e: { payload: T }) => void,
  enabled: boolean = true,
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    let unlistenFn: (() => void) | null = null;
    listen(event, (e: any) => {
      if (cancelled) return;
      handlerRef.current(e);
    }).then((fn) => {
      if (cancelled) { fn(); return; }
      unlistenFn = fn;
    });
    return () => {
      cancelled = true;
      unlistenFn?.();
    };
  }, [event, enabled]);
}

interface UseChatMessagesOptions {
  automationId: number | null;
  isPlaying: boolean;
}

export function useChatMessages({ automationId, isPlaying }: UseChatMessagesOptions) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [currentRunId, setCurrentRunId] = useState<number | undefined>(undefined);
  const [currentAutomationId, setCurrentAutomationId] = useState<number | undefined>(undefined);
  const stepCountRef = useRef(0);

  const addMessage = useCallback((msg: ChatMessage) => {
    setMessages((prev) => [...prev, msg]);
  }, []);

  const updateMessage = useCallback((id: string, updates: Partial<ChatMessage>) => {
    setMessages((prev) =>
      prev.map((m) => (m.id === id ? { ...m, ...updates } : m))
    );
  }, []);

  const updateLastMessageOfType = useCallback(
    (type: ChatMessageType, updates: Partial<ChatMessage>) => {
      setMessages((prev) => {
        const idx = [...prev].reverse().findIndex((m) => m.type === type);
        if (idx === -1) return prev;
        const realIdx = prev.length - 1 - idx;
        const updated = [...prev];
        updated[realIdx] = { ...updated[realIdx], ...updates };
        return updated;
      });
    },
    []
  );

  const clearMessages = useCallback(() => {
    setMessages([]);
    setCurrentRunId(undefined);
    setCurrentAutomationId(undefined);
    stepCountRef.current = 0;
  }, []);

  const removePlanningMessage = useCallback(() => {
    setMessages((prev) => {
      // Find last planning message (search backwards)
      let idx = -1;
      for (let i = prev.length - 1; i >= 0; i--) {
        if (prev[i].type === "planning") { idx = i; break; }
      }
      if (idx === -1) return prev;
      return [...prev.slice(0, idx), ...prev.slice(idx + 1)];
    });
  }, []);

  // Listen for automation_started — capture run ID and automation ID
  useEffect(() => {
    const unlisten = listen("automation_started", (event: any) => {
      if (event.payload?.execution_run_id) {
        setCurrentRunId(event.payload.execution_run_id);
      }
      if (event.payload?.automation_id) {
        setCurrentAutomationId(event.payload.automation_id);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Listen for agent_phase_started
  useSafeTauriListener("agent_phase_started", (event: any) => {
    const { phaseName, phaseNumber, goal, steps, nextPhaseHint } = event.payload;
    // Remove any lingering planning indicator (e.g. from continuation)
    removePlanningMessage();
    const metadata: PhasePlanMetadata = {
      phaseName,
      phaseNumber,
      goal,
      steps: steps || [],
      nextPhaseHint,
    };
    addMessage(
      createMessage("phase_plan", "assistant", `Phase ${phaseNumber}: ${phaseName}`, metadata)
    );
  }, isPlaying);

  // Listen for automation_step
  useSafeTauriListener("automation_step", (event: any) => {
    const { automationId: eventId, explanation, nextStep, status, error } = event.payload;
    if (automationId !== null && eventId !== automationId) return;

    // Remove any lingering planning indicator (e.g. from continuation)
    removePlanningMessage();

    // Mark previous executing step as completed
    setMessages((prev) => {
      const updated = [...prev];
      for (let i = updated.length - 1; i >= 0; i--) {
        if (
          updated[i].type === "execution_step" &&
          (updated[i].metadata as ExecutionStepMetadata)?.status === "executing"
        ) {
          updated[i] = {
            ...updated[i],
            metadata: { ...updated[i].metadata, status: "completed" },
          };
          break;
        }
      }
      return updated;
    });

    stepCountRef.current += 1;
    const metadata: ExecutionStepMetadata = {
      explanation: explanation || "Executing...",
      nextStep,
      status: status || "executing",
      error,
    };
    addMessage(
      createMessage(
        "execution_step",
        "assistant",
        explanation || "Executing...",
        metadata
      )
    );
  }, isPlaying);

  // Listen for terminal-process-started — attach to last step
  useEffect(() => {
    const unlisten = listen("terminal-process-started", (event: any) => {
      const { process_id, command } = event.payload;
      setMessages((prev) => {
        const updated = [...prev];
        for (let i = updated.length - 1; i >= 0; i--) {
          if (
            updated[i].type === "execution_step" &&
            (updated[i].metadata as ExecutionStepMetadata)?.status === "executing"
          ) {
            updated[i] = {
              ...updated[i],
              metadata: {
                ...updated[i].metadata,
                terminalProcessId: process_id,
                terminalCommand: command,
              },
            };
            break;
          }
        }
        return updated;
      });
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Listen for automation_completion_message
  useEffect(() => {
    const unlisten = listen("automation_completion_message", (event: any) => {
      const { message } = event.payload;

      // Mark last executing step as completed
      setMessages((prev) => {
        const updated = [...prev];
        for (let i = updated.length - 1; i >= 0; i--) {
          if (
            updated[i].type === "execution_step" &&
            (updated[i].metadata as ExecutionStepMetadata)?.status === "executing"
          ) {
            updated[i] = {
              ...updated[i],
              metadata: { ...updated[i].metadata, status: "completed" },
            };
            break;
          }
        }
        return updated;
      });

      const metadata: CompletionMetadata = {
        stepCount: stepCountRef.current,
      };
      addMessage(createMessage("completion", "assistant", message, metadata));
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addMessage]);

  // Listen for automation_stopped
  useEffect(() => {
    const unlisten = listen("automation_stopped", (event: any) => {
      // Mark last executing step as completed
      setMessages((prev) => {
        const updated = [...prev];
        for (let i = updated.length - 1; i >= 0; i--) {
          if (
            updated[i].type === "execution_step" &&
            (updated[i].metadata as ExecutionStepMetadata)?.status === "executing"
          ) {
            updated[i] = {
              ...updated[i],
              metadata: { ...updated[i].metadata, status: "completed" },
            };
            break;
          }
        }
        return updated;
      });

      if (event.payload?.clipboard) {
        // Update completion message if one exists, otherwise add stopped
        setMessages((prev) => {
          const lastCompletion = [...prev].reverse().find((m) => m.type === "completion");
          if (lastCompletion) {
            return prev.map((m) =>
              m.id === lastCompletion.id
                ? { ...m, metadata: { ...m.metadata, clipboardContent: event.payload.clipboard } }
                : m
            );
          }
          return prev;
        });
      }

      // Only add a "stopped" message if there's no completion message
      setMessages((prev) => {
        const hasCompletion = prev.some((m) => m.type === "completion");
        if (!hasCompletion) {
          return [...prev, createMessage("stopped", "system", "Task stopped")];
        }
        return prev;
      });
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Listen for automation_failed — clears in-progress UI when the backend
  // aborts due to an error (e.g. network failure sending to the LLM). Without
  // this, lingering "planning"/"executing" bubbles keep spinning forever.
  useEffect(() => {
    const unlisten = listen("automation_failed", () => {
      removePlanningMessage();
      setMessages((prev) => {
        const updated = [...prev];
        for (let i = updated.length - 1; i >= 0; i--) {
          if (
            updated[i].type === "execution_step" &&
            (updated[i].metadata as ExecutionStepMetadata)?.status === "executing"
          ) {
            updated[i] = {
              ...updated[i],
              metadata: { ...updated[i].metadata, status: "completed" },
            };
            break;
          }
        }
        const hasTerminal = updated.some(
          (m) => m.type === "completion" || m.type === "stopped" || m.type === "error"
        );
        if (!hasTerminal) {
          updated.push(createMessage("error", "system", "Automation failed"));
        }
        return updated;
      });
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [removePlanningMessage]);

  // Listen for clarification_required
  useEffect(() => {
    const unlisten = listen("clarification_required", (event: any) => {
      const metadata: ClarificationMetadata = {
        question: event.payload.question || "Please provide clarification",
        reasoning: event.payload.reasoning || "",
        responded: false,
      };
      addMessage(
        createMessage("clarification_ask", "assistant", metadata.question, metadata)
      );
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addMessage]);

  // Listen for user_takeover_required
  useEffect(() => {
    const unlisten = listen("user_takeover_required", (event: any) => {
      const metadata: TakeoverMetadata = {
        instructions: event.payload.instructions || "Please complete the required action",
        reasoning: event.payload.reasoning || "",
        completed: false,
      };
      addMessage(
        createMessage("takeover_request", "assistant", metadata.instructions, metadata)
      );
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [addMessage]);

  // Cloud delegation listeners (delegation_started / delegation_step /
  // delegation_takeover_required / delegation_takeover_resolved /
  // delegation_completed) intentionally omitted in the open-source build —
  // there is no cloud_executor or DELEGATE_TO_CLOUD action wired in.

  return {
    messages,
    setMessages,
    currentRunId,
    setCurrentRunId,
    currentAutomationId,
    setCurrentAutomationId,
    addMessage,
    updateMessage,
    updateLastMessageOfType,
    clearMessages,
    removePlanningMessage,
    stepCount: stepCountRef.current,
  };
}

import { ChatMessage, createMessage, CompletionMetadata, ExecutionStepMetadata } from "../chatTypes";
import { ExecutionRunWithSteps } from "../components/ExecutionHistoryView";

export function executionToMessages(
  execution: ExecutionRunWithSteps,
  objective: string
): ChatMessage[] {
  const messages: ChatMessage[] = [];

  // Initial user prompt
  if (objective) {
    messages.push(createMessage("user_prompt", "user", objective));
  }

  // Process steps into conversation segments
  for (const step of execution.steps) {
    if (step.step_type === "user_prompt") {
      // User continuation message
      messages.push(createMessage("user_prompt", "user", step.explanation || ""));
    } else if (step.step_type === "quick_reply") {
      // Conversational assistant reply (no automation work)
      messages.push(createMessage("quick_reply", "assistant", step.explanation || ""));
    } else {
      // Execution step
      const meta: ExecutionStepMetadata = {
        explanation: step.explanation || step.next_step || "Executing...",
        nextStep: step.next_step || undefined,
        status: step.status === "error" || step.status === "failed" ? "error" : "completed",
        error: step.error_message || undefined,
      };
      messages.push(
        createMessage("execution_step", "assistant", meta.explanation, meta)
      );
    }
  }

  // Completion message
  if (execution.run.completion_message && execution.run.status === "completed") {
    const completionMeta: CompletionMetadata = {
      stepCount: execution.steps.filter((s) => s.step_type !== "user_prompt" && s.step_type !== "quick_reply").length,
      clipboardContent: execution.run.clipboard,
    };
    messages.push(
      createMessage("completion", "assistant", execution.run.completion_message, completionMeta)
    );
  }

  // Error message
  if (execution.run.error_message) {
    messages.push(createMessage("error", "assistant", execution.run.error_message));
  }

  // Stopped message
  if (execution.run.status === "stopped" && !execution.run.completion_message) {
    messages.push(createMessage("stopped", "system", "Task stopped by user"));
  }

  return messages;
}

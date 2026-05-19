import React from "react";
import { ChatMessage } from "../chatTypes";
import { UserMessageBubble } from "./chat/UserMessageBubble";
import { PlanningIndicator } from "./chat/PlanningIndicator";
import { PhasePlanBubble } from "./chat/PhasePlanBubble";
import { ExecutionStepBubble } from "./chat/ExecutionStepBubble";
import { ClarificationInline } from "./chat/ClarificationInline";
import { TakeoverInline } from "./chat/TakeoverInline";
import { CompletionBubble } from "./chat/CompletionBubble";
import { SystemMessageBubble } from "./chat/SystemMessageBubble";

interface ChatMessageRendererProps {
  message: ChatMessage;
  isLatest: boolean;
  onSubmitClarification: (response: string) => void;
  onCompleteTakeover: () => void;
}

export const ChatMessageRenderer: React.FC<ChatMessageRendererProps> = ({
  message,
  isLatest,
  onSubmitClarification,
  onCompleteTakeover,
}) => {
  switch (message.type) {
    case "user_prompt":
    case "clarification_response":
      return <UserMessageBubble message={message} />;

    case "planning":
      return <PlanningIndicator message={message} />;

    case "phase_plan":
      return <PhasePlanBubble message={message} />;

    case "execution_step":
      return <ExecutionStepBubble message={message} />;

    case "clarification_ask":
      return (
        <ClarificationInline
          message={message}
          isLatest={isLatest}
          onSubmit={onSubmitClarification}
        />
      );

    case "takeover_request":
      return (
        <TakeoverInline
          message={message}
          isLatest={isLatest}
          onComplete={onCompleteTakeover}
        />
      );

    case "quick_reply":
    case "completion":
      return <CompletionBubble message={message} />;

    case "error":
      return <CompletionBubble message={{ ...message, metadata: { ...message.metadata } }} />;

    case "takeover_complete":
    case "stopped":
    case "system":
      return <SystemMessageBubble message={message} />;

    default:
      return null;
  }
};

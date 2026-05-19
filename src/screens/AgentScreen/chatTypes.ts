export type ChatMessageType =
  | 'user_prompt'
  | 'planning'
  | 'phase_plan'
  | 'execution_step'
  | 'clarification_ask'
  | 'clarification_response'
  | 'takeover_request'
  | 'takeover_complete'
  | 'quick_reply'
  | 'completion'
  | 'error'
  | 'stopped'
  | 'system'
  | 'delegation'
  | 'delegation_takeover';

export interface ChatMessage {
  id: string;
  type: ChatMessageType;
  role: 'user' | 'assistant' | 'system';
  timestamp: string;
  content: string;
  metadata?: Record<string, any>;
}

export interface PlanningMetadata {
  agentMode: boolean;
}

export interface PhasePlanMetadata {
  phaseName: string;
  phaseNumber: number;
  goal: string;
  steps: string[];
  nextPhaseHint?: string;
}

export interface ExecutionStepMetadata {
  explanation: string;
  nextStep?: string;
  status: 'executing' | 'completed' | 'error';
  error?: string;
  terminalProcessId?: string;
  terminalCommand?: string;
}

export interface ClarificationMetadata {
  question: string;
  reasoning: string;
  responded: boolean;
}

export interface TakeoverMetadata {
  instructions: string;
  reasoning: string;
  completed: boolean;
}

export interface CompletionMetadata {
  clipboardContent?: string;
  stepCount: number;
}

export interface DelegationStep {
  stepIndex?: number;
  actionType?: string;
  explanation: string;
  status?: string;
  error?: string;
}

export interface DelegationMetadata {
  taskBrief: string;
  status: 'running' | 'completed' | 'failed' | 'stopped' | 'timeout';
  steps: DelegationStep[];
  summary?: string;
  durationSeconds?: number;
  stepCount?: number;
  proxyExecutionRunId?: string;
  localRunId?: number;
  vmIp?: string;
  hostname?: string;
  /** Cloud sub-run linked to its parent local run for cross-reference. */
  parentLocalRunId?: number | null;
  /** Truncated JSON stringified extracted_data from the cloud, if any. */
  extractedDataPreview?: string;
  /** Accumulated MEMORY_SAVE text from the cloud — the actual deliverable
   *  (scraped data, jokes, research notes). Truncated for display. */
  memoryPreview?: string;
}

export interface DelegationTakeoverMetadata {
  proxyExecutionRunId: string;
  localRunId?: number;
  parentAutomationId?: number;
  instructions?: string;   // What the cloud wants the user to do
  reasoning?: string;      // Why the cloud is asking
  startedAt?: string;
  /** Set true once the user has clicked "Resume" — the bubble dims, awaits cloud confirmation. */
  awaitingResume?: boolean;
  /** Set true after the cloud confirms `delegation_takeover_resolved`. */
  resolved?: boolean;
}

export function createMessage(
  type: ChatMessageType,
  role: 'user' | 'assistant' | 'system',
  content: string,
  metadata?: Record<string, any>,
): ChatMessage {
  return {
    id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
    type,
    role,
    timestamp: new Date().toISOString(),
    content,
    metadata,
  };
}

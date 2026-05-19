import React, { useRef, useEffect, useCallback, useState, useMemo } from "react";
import {
  Flex,
  Box,
  VStack,
  HStack,
  Image,
  IconButton,
  Collapse,
  Badge,
  Button,
  Tooltip,
  Spinner,
  Text as ChakraText,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";
import { keyframes } from "@emotion/react";
import { ArrowDown, ArrowRight, ChevronDown, ChevronRight, CheckCircle, AlertCircle, Clock, Square, Play, Edit, CalendarClock } from "lucide-react";
import logoBlack from "@heelix-app/design/logo/logo-black.png";
import { ChatMessage } from "../chatTypes";
import { ChatMessageRenderer } from "./ChatMessageRenderer";
import { ChatInputBar } from "./chat/ChatInputBar";
import { CATEGORY_PROMPTS } from "../categoryPrompts";

const pulse = keyframes`
  0% { opacity: 0.7; transform: scale(0.95); }
  50% { opacity: 1; transform: scale(1.05); }
  100% { opacity: 0.7; transform: scale(0.95); }
`;

type InputMode = "idle" | "planning" | "executing" | "completed";

// A collapsible step types
const STEP_TYPES = new Set(["execution_step", "phase_plan"]);
// Types that trigger collapsing the preceding steps
const TERMINAL_TYPES = new Set(["completion", "stopped", "error"]);

/**
 * Groups messages into render items. When a completion/stopped/error message
 * follows a run of execution steps, those steps are grouped into a collapsible
 * block. Steps that are still streaming (no terminal message yet) render individually.
 */
type RenderItem =
  | { kind: "message"; message: ChatMessage; index: number }
  | { kind: "collapsed_steps"; steps: ChatMessage[]; id: string };

function groupMessages(messages: ChatMessage[]): RenderItem[] {
  const items: RenderItem[] = [];
  let stepBuffer: ChatMessage[] = [];

  const flushStepsAsCollapsed = () => {
    if (stepBuffer.length > 0) {
      items.push({
        kind: "collapsed_steps",
        steps: [...stepBuffer],
        id: `steps-${stepBuffer[0].id}`,
      });
      stepBuffer = [];
    }
  };

  const flushStepsAsIndividual = () => {
    for (const step of stepBuffer) {
      items.push({ kind: "message", message: step, index: 0 });
    }
    stepBuffer = [];
  };

  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];

    if (STEP_TYPES.has(msg.type)) {
      // Accumulate steps
      stepBuffer.push(msg);
    } else if (TERMINAL_TYPES.has(msg.type)) {
      // Terminal message — collapse preceding steps
      flushStepsAsCollapsed();
      items.push({ kind: "message", message: msg, index: i });
    } else {
      // Non-step, non-terminal (user_prompt, planning, clarification, system, etc.)
      // If there were accumulated steps without a terminal, they're still live — flush individually
      flushStepsAsIndividual();
      items.push({ kind: "message", message: msg, index: i });
    }
  }

  // Any remaining steps at the end are still streaming — show them individually
  flushStepsAsIndividual();

  // Fix indices for proper isLatest detection
  let lastIdx = 0;
  for (const item of items) {
    if (item.kind === "message") {
      item.index = lastIdx++;
    } else {
      lastIdx++;
    }
  }

  return items;
}

/** Collapsible step group */
const CollapsedStepGroup: React.FC<{
  steps: ChatMessage[];
  onSubmitClarification: (response: string) => void;
  onCompleteTakeover: () => void;
}> = ({ steps, onSubmitClarification, onCompleteTakeover }) => {
  const [isExpanded, setIsExpanded] = useState(false);

  return (
    <Box mb={2}>
      <HStack
        spacing={2}
        cursor="pointer"
        onClick={() => setIsExpanded(!isExpanded)}
        py={2}
        px={3}
        _hover={{ bg: "gray.100" }}
        borderRadius="md"
        transition="background 0.15s"
      >
        {isExpanded ? (
          <ChevronDown size={14} color="#6b7280" />
        ) : (
          <ChevronRight size={14} color="#6b7280" />
        )}
        <ChakraText
          fontSize="xs"
          fontWeight="semibold"
          color="gray.500"
          textTransform="uppercase"
          letterSpacing="wide"
        >
          {steps.length} steps executed
        </ChakraText>
      </HStack>
      <Collapse in={isExpanded} animateOpacity>
        <Box pl={4} pt={1}>
          {steps.map((msg, idx) => (
            <ChatMessageRenderer
              key={msg.id}
              message={msg}
              isLatest={false}
              onSubmitClarification={onSubmitClarification}
              onCompleteTakeover={onCompleteTakeover}
            />
          ))}
        </Box>
      </Collapse>
    </Box>
  );
};

interface ExecutionHeaderInfo {
  taskName: string;
  status: string;
  startedAt: string;
  completedAt?: string;
  automationId: number;
  executionRunId: number;
  additionalInstructions?: string;
}

interface ChatViewProps {
  messages: ChatMessage[];
  isPlaying: boolean;
  isPlanning: boolean;
  onSendPrompt: (text: string, agentMode: boolean) => void;
  onSendMessage: (text: string) => void;
  onStop: () => void;
  onContinue: (text: string) => void;
  onSubmitClarification: (response: string) => void;
  onCompleteTakeover: () => void;
  executionHeader?: ExecutionHeaderInfo;
  onEdit?: (automationId: number) => void;
  onSchedule?: (automationId: number, executionRunId: number) => void;
  onRerun?: (automationId: number, additionalInstructions?: string) => void;
}

const getStatusIcon = (status: string) => {
  switch (status) {
    case "completed":
      return <CheckCircle size={18} color="#22c55e" />;
    case "running":
    case "executing":
      return <Spinner size="sm" color="blue.500" />;
    case "failed":
    case "error":
      return <AlertCircle size={18} color="#ef4444" />;
    case "stopped":
      return <Square size={18} color="#9ca3af" />;
    default:
      return <Clock size={18} color="#9ca3af" />;
  }
};

const getStatusColor = (status: string) => {
  switch (status) {
    case "completed": return "green";
    case "running":
    case "executing": return "blue";
    case "failed":
    case "error": return "red";
    default: return "gray";
  }
};

const formatDuration = (startedAt: string, completedAt?: string) => {
  const start = new Date(startedAt).getTime();
  const end = completedAt ? new Date(completedAt).getTime() : Date.now();
  const seconds = Math.floor((end - start) / 1000);
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes > 0) return `${minutes}m ${remainingSeconds}s`;
  return `${seconds}s`;
};

export const ChatView: React.FC<ChatViewProps> = ({
  messages,
  isPlaying,
  isPlanning,
  onSendPrompt,
  onSendMessage,
  onStop,
  onContinue,
  onSubmitClarification,
  onCompleteTakeover,
  executionHeader,
  onEdit,
  onSchedule,
  onRerun,
}) => {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const [agentMode, setAgentMode] = useState(false);

  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);

  // Group messages for rendering (steps collapse after completion)
  const renderItems = useMemo(() => groupMessages(messages), [messages]);

  // Determine input mode
  const inputMode: InputMode = (() => {
    if (messages.length === 0) return "idle";
    if (isPlanning) return "planning";
    if (isPlaying) return "executing";
    // Find the last meaningful assistant/system message to determine mode
    const lastMeaningful = [...messages]
      .reverse()
      .find((m) =>
        m.type === "completion" || m.type === "stopped" || m.type === "error" || m.type === "quick_reply"
      );
    if (!lastMeaningful) return "idle";
    // After a quick_reply, stay in idle mode (ready for new prompt, not continuation)
    if (lastMeaningful.type === "quick_reply") return "idle";
    return "completed";
  })();

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    if (isAtBottom && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, isAtBottom]);

  // Track scroll position
  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    setIsAtBottom(scrollHeight - scrollTop - clientHeight < 60);
  }, []);

  const scrollToBottom = () => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
      setIsAtBottom(true);
    }
  };

  // Resolve whether a render item is the last one
  const totalItems = renderItems.length;

  // Empty state — input centered like Claude/ChatGPT
  if (messages.length === 0) {
    return (
      <Flex direction="column" width="100%" height="100%">
        {/* Fixed section: logo + input + category pills — pushed down ~40% from top */}
        <Flex
          direction="column"
          align="center"
          px={6}
          pt="18vh"
          flexShrink={0}
        >
          <VStack spacing={6} width="100%" maxW="680px">
            {/* Logo + heading inline */}
            <HStack spacing={3} align="center">
              <Image
                width="36px"
                height="36px"
                src={logoBlack}
                sx={{
                  opacity: 0.85,
                  animation: `${pulse} 3s infinite ease-in-out`,
                }}
              />
              <ChakraText fontSize="xl" fontWeight="bold" color="gray.800">
                What can I help with?
              </ChakraText>
            </HStack>

            {/* Input bar */}
            <ChatInputBar
              mode="idle"
              onSendPrompt={onSendPrompt}
              onSendMessage={onSendMessage}
              onContinue={onContinue}
              onStop={onStop}
              agentMode={agentMode}
              onAgentModeChange={setAgentMode}
              centered
            />

            {/* Category pills */}
            <HStack spacing={2} justify="center" flexWrap="wrap">
              {Object.entries(CATEGORY_PROMPTS).map(([key, cat]) => {
                const Icon = cat.icon;
                const isSelected = selectedCategory === key;
                return (
                  <Flex
                    key={key}
                    as="button"
                    px={3}
                    py={1.5}
                    bg={isSelected ? "blue.50" : "white"}
                    border="1px solid"
                    borderColor={isSelected ? "blue.400" : "gray.200"}
                    borderRadius="full"
                    fontSize="sm"
                    color={isSelected ? "blue.600" : "gray.600"}
                    align="center"
                    gap={1.5}
                    _hover={{ borderColor: "blue.400", bg: "blue.50", color: "blue.600" }}
                    onClick={() => setSelectedCategory(isSelected ? null : key)}
                    transition="all 0.15s"
                  >
                    <Icon size={13} />
                    <ChakraText fontSize="sm">{cat.label}</ChakraText>
                  </Flex>
                );
              })}
            </HStack>
          </VStack>
        </Flex>

        {/* Scrollable section: prompt cards flow below without pushing the input */}
        {selectedCategory && CATEGORY_PROMPTS[selectedCategory] && (
          <Flex
            direction="column"
            align="center"
            px={6}
            pt={4}
            pb={6}
            flex={1}
            overflow="auto"
          >
            <VStack spacing={2} w="full" maxW="600px">
              {CATEGORY_PROMPTS[selectedCategory].prompts.map((prompt, idx) => (
                <Flex
                  key={idx}
                  as="button"
                  w="full"
                  px={4}
                  py={3}
                  bg="white"
                  border="1px solid"
                  borderColor="gray.200"
                  borderRadius="xl"
                  fontSize="sm"
                  color="gray.700"
                  align="center"
                  justify="space-between"
                  _hover={{ borderColor: "blue.400", bg: "blue.50" }}
                  _active={{ bg: "blue.100" }}
                  onClick={() => onSendPrompt(prompt, agentMode)}
                  transition="all 0.15s"
                  textAlign="left"
                >
                  <ChakraText flex={1} fontSize="sm">{prompt}</ChakraText>
                  <Box ml={2} flexShrink={0} transform="rotate(-45deg)">
                    <ArrowRight size={14} />
                  </Box>
                </Flex>
              ))}
            </VStack>
          </Flex>
        )}
      </Flex>
    );
  }

  const canSchedule = executionHeader && (executionHeader.status === "completed" || executionHeader.status === "stopped");

  // Conversation state
  return (
    <Flex direction="column" width="100%" height="100%">
      {/* Execution header bar (history view) */}
      {executionHeader && (
        <Box px={6} py={4} borderBottom="1px solid" borderColor="gray.200" bg="white">
          <HStack justify="space-between" align="center">
            <HStack spacing={3} minW={0} flex={1}>
              {getStatusIcon(executionHeader.status)}
              <Text type="l" bold>
                {executionHeader.taskName || "Task Details"}
              </Text>
              <Badge colorScheme={getStatusColor(executionHeader.status)} size="sm">
                {executionHeader.status}
              </Badge>
              {executionHeader.completedAt && (
                <ChakraText fontSize="xs" color="gray.500" flexShrink={0}>
                  {formatDuration(executionHeader.startedAt, executionHeader.completedAt)}
                </ChakraText>
              )}
            </HStack>
            <HStack spacing={2} flexShrink={0}>
              {onEdit && (
                <Tooltip label="Edit task plan">
                  <Button
                    size="sm"
                    leftIcon={<Edit size={14} />}
                    variant="outline"
                    onClick={() => onEdit(executionHeader.automationId)}
                  >
                    Edit
                  </Button>
                </Tooltip>
              )}
              {onSchedule && canSchedule && (
                <Tooltip label="Run on a schedule">
                  <Button
                    size="sm"
                    leftIcon={<CalendarClock size={14} />}
                    variant="outline"
                    onClick={() => onSchedule(executionHeader.automationId, executionHeader.executionRunId)}
                  >
                    Schedule
                  </Button>
                </Tooltip>
              )}
              {onRerun && (
                <Tooltip label="Run this task again">
                  <Button
                    size="sm"
                    leftIcon={<Play size={14} />}
                    colorScheme="blue"
                    onClick={() => onRerun(executionHeader.automationId, executionHeader.additionalInstructions)}
                  >
                    Run Again
                  </Button>
                </Tooltip>
              )}
            </HStack>
          </HStack>
        </Box>
      )}

      {/* Message list */}
      <Box
        ref={scrollRef}
        flex={1}
        overflow="auto"
        p={4}
        bg="gray.50"
        onScroll={handleScroll}
        sx={{
          "&::-webkit-scrollbar": { width: "6px" },
          "&::-webkit-scrollbar-track": { background: "rgba(0, 0, 0, 0.05)" },
          "&::-webkit-scrollbar-thumb": {
            background: "rgba(0, 0, 0, 0.2)",
            borderRadius: "5px",
          },
        }}
      >
        <VStack spacing={0} align="stretch" maxW="700px" mx="auto" pb={4}>
          {renderItems.map((item, idx) => {
            if (item.kind === "collapsed_steps") {
              return (
                <CollapsedStepGroup
                  key={item.id}
                  steps={item.steps}
                  onSubmitClarification={onSubmitClarification}
                  onCompleteTakeover={onCompleteTakeover}
                />
              );
            }
            return (
              <ChatMessageRenderer
                key={item.message.id}
                message={item.message}
                isLatest={idx === totalItems - 1}
                onSubmitClarification={onSubmitClarification}
                onCompleteTakeover={onCompleteTakeover}
              />
            );
          })}
        </VStack>
      </Box>

      {/* Scroll to bottom button */}
      {!isAtBottom && (
        <Flex justify="center" position="relative">
          <IconButton
            aria-label="Scroll to bottom"
            icon={<ArrowDown size={16} />}
            size="sm"
            borderRadius="full"
            position="absolute"
            bottom="8px"
            boxShadow="md"
            bg="white"
            onClick={scrollToBottom}
            zIndex={1}
          />
        </Flex>
      )}

      {/* Input bar */}
      <ChatInputBar
        mode={inputMode}
        onSendPrompt={onSendPrompt}
        onSendMessage={onSendMessage}
        onContinue={onContinue}
        onStop={onStop}
        agentMode={agentMode}
        onAgentModeChange={setAgentMode}
      />
    </Flex>
  );
};

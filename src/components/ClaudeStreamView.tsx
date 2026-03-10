/**
 * ClaudeStreamView - Rich display for Claude CLI streaming output
 * 
 * Displays:
 * - Thinking/reasoning in a collapsible section
 * - Tool calls with status indicators
 * - Main content/response
 */

import React, { useState, useEffect, useRef, useMemo } from "react";
import {
  Box,
  VStack,
  HStack,
  Text,
  Collapse,
  IconButton,
  Badge,
  Spinner,
  Tooltip,
  Code,
} from "@chakra-ui/react";
import {
  Brain,
  ChevronDown,
  ChevronRight,
  Terminal,
  CheckCircle,
  AlertCircle,
  Play,
  Copy,
  Check,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import {
  ClaudeStreamAssembler,
  ClaudeDisplayState,
  ToolCall,
  ClaudeStreamEvent,
  ClaudeThinkingEvent,
  ClaudeContentEvent,
  ClaudeToolEvent,
  ClaudeCliDetectedEvent,
  createClaudeStreamAssembler,
} from "../utils/claudeStreamAssembler";

interface ClaudeStreamViewProps {
  /** Process ID to filter events for */
  processId: string;
  /** Whether to show thinking by default */
  showThinking?: boolean;
  /** Max height for scrollable areas */
  maxHeight?: string;
  /** Command that's running (for display) */
  command?: string;
}

// Tool status badge component
const ToolStatusBadge: React.FC<{ status: ToolCall["status"] }> = ({ status }) => {
  switch (status) {
    case "started":
      return <Badge colorScheme="blue" fontSize="10px">Starting</Badge>;
    case "running":
      return (
        <Badge colorScheme="yellow" fontSize="10px">
          <HStack spacing={1}>
            <Spinner size="xs" />
            <span>Running</span>
          </HStack>
        </Badge>
      );
    case "completed":
      return (
        <Badge colorScheme="green" fontSize="10px">
          <HStack spacing={1}>
            <CheckCircle size={10} />
            <span>Done</span>
          </HStack>
        </Badge>
      );
    case "error":
      return (
        <Badge colorScheme="red" fontSize="10px">
          <HStack spacing={1}>
            <AlertCircle size={10} />
            <span>Error</span>
          </HStack>
        </Badge>
      );
    default:
      return null;
  }
};

// Tool card component
const ToolCard: React.FC<{ tool: ToolCall }> = ({ tool }) => {
  const [isExpanded, setIsExpanded] = useState(tool.status === "running");
  const [copied, setCopied] = useState(false);

  // Format tool input for display
  const formattedInput = useMemo(() => {
    if (!tool.input) return null;
    try {
      if (typeof tool.input === "string") return tool.input;
      return JSON.stringify(tool.input, null, 2);
    } catch {
      return String(tool.input);
    }
  }, [tool.input]);

  const handleCopy = async () => {
    const textToCopy = tool.result || formattedInput || "";
    await navigator.clipboard.writeText(textToCopy);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // Get icon based on tool name
  const getToolIcon = () => {
    const name = tool.name.toLowerCase();
    if (name.includes("bash") || name.includes("shell") || name.includes("command")) {
      return <Terminal size={14} />;
    }
    return <Play size={14} />;
  };

  return (
    <Box
      bg="gray.800"
      borderRadius="md"
      border="1px solid"
      borderColor={
        tool.status === "running"
          ? "blue.400"
          : tool.status === "error"
          ? "red.400"
          : "gray.600"
      }
      overflow="hidden"
    >
      {/* Tool header */}
      <HStack
        px={3}
        py={2}
        bg="gray.750"
        cursor="pointer"
        onClick={() => setIsExpanded(!isExpanded)}
        justify="space-between"
        _hover={{ bg: "gray.700" }}
      >
        <HStack spacing={2}>
          <IconButton
            aria-label="Toggle tool details"
            icon={isExpanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
            size="xs"
            variant="ghost"
            color="gray.400"
            _hover={{ bg: "transparent" }}
          />
          <Box color="blue.300">{getToolIcon()}</Box>
          <Text fontSize="xs" color="blue.300" fontWeight="bold" fontFamily="mono">
            {tool.name}
          </Text>
          <ToolStatusBadge status={tool.status} />
        </HStack>

        <Tooltip label={copied ? "Copied!" : "Copy output"}>
          <IconButton
            aria-label="Copy output"
            icon={copied ? <Check size={12} /> : <Copy size={12} />}
            size="xs"
            variant="ghost"
            color="gray.400"
            _hover={{ color: "white", bg: "gray.700" }}
            onClick={(e) => {
              e.stopPropagation();
              handleCopy();
            }}
          />
        </Tooltip>
      </HStack>

      {/* Tool details */}
      <Collapse in={isExpanded} animateOpacity>
        <Box p={3} fontSize="xs" fontFamily="mono">
          {/* Input */}
          {formattedInput && (
            <Box mb={2}>
              <Text color="gray.500" mb={1}>Input:</Text>
              <Code
                display="block"
                p={2}
                bg="gray.900"
                borderRadius="sm"
                whiteSpace="pre-wrap"
                wordBreak="break-all"
                color="gray.300"
                fontSize="11px"
                maxH="100px"
                overflowY="auto"
              >
                {formattedInput.length > 500
                  ? formattedInput.slice(0, 500) + "..."
                  : formattedInput}
              </Code>
            </Box>
          )}

          {/* Result */}
          {tool.result && (
            <Box>
              <Text color="gray.500" mb={1}>Output:</Text>
              <Code
                display="block"
                p={2}
                bg="gray.900"
                borderRadius="sm"
                whiteSpace="pre-wrap"
                wordBreak="break-all"
                color={tool.status === "error" ? "red.300" : "green.300"}
                fontSize="11px"
                maxH="150px"
                overflowY="auto"
              >
                {tool.result.length > 1000
                  ? tool.result.slice(0, 1000) + "..."
                  : tool.result}
              </Code>
            </Box>
          )}

          {/* Running indicator */}
          {tool.status === "running" && !tool.result && (
            <HStack spacing={2} color="blue.300">
              <Spinner size="xs" />
              <Text fontSize="11px">Executing...</Text>
            </HStack>
          )}
        </Box>
      </Collapse>
    </Box>
  );
};

// Thinking section component
const ThinkingSection: React.FC<{
  thinking: string;
  isCollapsed: boolean;
  onToggle: () => void;
}> = ({ thinking, isCollapsed, onToggle }) => {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);

  // Auto-scroll thinking as it updates
  useEffect(() => {
    if (scrollRef.current && !isCollapsed) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [thinking, isCollapsed]);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(thinking);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (!thinking) return null;

  return (
    <Box
      bg="purple.900"
      borderRadius="md"
      border="1px solid"
      borderColor="purple.600"
      overflow="hidden"
    >
      {/* Header */}
      <HStack
        px={3}
        py={2}
        bg="purple.800"
        cursor="pointer"
        onClick={onToggle}
        justify="space-between"
        _hover={{ bg: "purple.700" }}
      >
        <HStack spacing={2}>
          <IconButton
            aria-label="Toggle thinking"
            icon={isCollapsed ? <ChevronRight size={12} /> : <ChevronDown size={12} />}
            size="xs"
            variant="ghost"
            color="purple.200"
            _hover={{ bg: "transparent" }}
          />
          <Brain size={14} color="#D6BCFA" />
          <Text fontSize="xs" color="purple.200" fontWeight="bold">
            Thinking...
          </Text>
          <Badge colorScheme="purple" fontSize="10px">
            {thinking.length} chars
          </Badge>
        </HStack>

        <Tooltip label={copied ? "Copied!" : "Copy thinking"}>
          <IconButton
            aria-label="Copy thinking"
            icon={copied ? <Check size={12} /> : <Copy size={12} />}
            size="xs"
            variant="ghost"
            color="purple.200"
            _hover={{ color: "white", bg: "purple.700" }}
            onClick={(e) => {
              e.stopPropagation();
              handleCopy();
            }}
          />
        </Tooltip>
      </HStack>

      {/* Content */}
      <Collapse in={!isCollapsed} animateOpacity>
        <Box
          ref={scrollRef}
          p={3}
          maxH="200px"
          overflowY="auto"
          sx={{
            "&::-webkit-scrollbar": { width: "6px" },
            "&::-webkit-scrollbar-track": { background: "rgba(0, 0, 0, 0.2)" },
            "&::-webkit-scrollbar-thumb": {
              background: "rgba(214, 188, 250, 0.3)",
              borderRadius: "3px",
            },
          }}
        >
          <Text
            fontSize="12px"
            color="purple.100"
            whiteSpace="pre-wrap"
            wordBreak="break-word"
            fontFamily="'JetBrains Mono', 'Fira Code', monospace"
            lineHeight="1.6"
          >
            {thinking}
          </Text>
        </Box>
      </Collapse>
    </Box>
  );
};

// Main component
export const ClaudeStreamView: React.FC<ClaudeStreamViewProps> = ({
  processId,
  showThinking = true,
  maxHeight = "400px",
  command,
}) => {
  const [state, setState] = useState<ClaudeDisplayState>({
    isClaudeCli: false,
    thinking: "",
    content: "",
    toolCalls: new Map(),
    hasError: false,
    isComplete: false,
    lastUpdated: Date.now(),
  });
  const [thinkingCollapsed, setThinkingCollapsed] = useState(false);
  const [copied, setCopied] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const assemblerRef = useRef<ClaudeStreamAssembler | null>(null);

  // Create assembler on mount
  useEffect(() => {
    assemblerRef.current = createClaudeStreamAssembler(processId);
    const unsubscribe = assemblerRef.current.subscribe(setState);
    return () => {
      unsubscribe();
      assemblerRef.current = null;
    };
  }, [processId]);

  // Listen for Claude events
  useEffect(() => {
    const assembler = assemblerRef.current;
    if (!assembler) return;

    // Listen for Claude CLI detected
    const unlistenDetected = listen<ClaudeCliDetectedEvent>(
      "claude-cli-detected",
      (event) => {
        assembler.handleCliDetected(event.payload);
      }
    );

    // Listen for thinking events
    const unlistenThinking = listen<ClaudeThinkingEvent>(
      "claude-thinking",
      (event) => {
        assembler.handleThinking(event.payload);
      }
    );

    // Listen for content events
    const unlistenContent = listen<ClaudeContentEvent>(
      "claude-content",
      (event) => {
        assembler.handleContent(event.payload);
      }
    );

    // Listen for tool events
    const unlistenTool = listen<ClaudeToolEvent>("claude-tool", (event) => {
      assembler.handleTool(event.payload);
    });

    // Listen for generic stream events
    const unlistenStream = listen<ClaudeStreamEvent>(
      "claude-stream",
      (event) => {
        assembler.handleStreamEvent(event.payload);
      }
    );

    return () => {
      unlistenDetected.then((fn) => fn());
      unlistenThinking.then((fn) => fn());
      unlistenContent.then((fn) => fn());
      unlistenTool.then((fn) => fn());
      unlistenStream.then((fn) => fn());
    };
  }, [processId]);

  // Auto-scroll content
  useEffect(() => {
    if (contentRef.current) {
      contentRef.current.scrollTop = contentRef.current.scrollHeight;
    }
  }, [state.content]);

  const handleCopyContent = async () => {
    await navigator.clipboard.writeText(state.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // Convert tool calls map to array for rendering
  const toolCallsArray = useMemo(
    () => Array.from(state.toolCalls.values()),
    [state.toolCalls]
  );

  // Don't render if not a Claude CLI process
  if (!state.isClaudeCli) {
    return null;
  }

  return (
    <Box
      bg="gray.900"
      borderRadius="md"
      overflow="hidden"
      border="1px solid"
      borderColor="gray.700"
    >
      {/* Header */}
      <HStack
        px={3}
        py={2}
        bg="gray.800"
        justify="space-between"
        borderBottom="1px solid"
        borderColor="gray.700"
      >
        <HStack spacing={2}>
          <Box
            w={2}
            h={2}
            borderRadius="full"
            bg={state.isComplete ? "green.400" : "blue.400"}
            animation={!state.isComplete ? "pulse 2s infinite" : undefined}
          />
          <Text fontSize="xs" color="gray.300" fontFamily="mono">
            {command
              ? `$ ${command.slice(0, 50)}${command.length > 50 ? "..." : ""}`
              : "Claude CLI"}
          </Text>
          {state.isComplete && (
            <Badge colorScheme="green" fontSize="10px">
              Complete
            </Badge>
          )}
          {state.hasError && (
            <Badge colorScheme="red" fontSize="10px">
              Error
            </Badge>
          )}
        </HStack>

        {state.content && (
          <Tooltip label={copied ? "Copied!" : "Copy response"}>
            <IconButton
              aria-label="Copy response"
              icon={copied ? <Check size={12} /> : <Copy size={12} />}
              size="xs"
              variant="ghost"
              color="gray.400"
              _hover={{ color: "white", bg: "gray.700" }}
              onClick={handleCopyContent}
            />
          </Tooltip>
        )}
      </HStack>

      {/* Main content area */}
      <Box
        maxH={maxHeight}
        overflowY="auto"
        p={3}
        sx={{
          "&::-webkit-scrollbar": { width: "6px" },
          "&::-webkit-scrollbar-track": { background: "rgba(0, 0, 0, 0.2)" },
          "&::-webkit-scrollbar-thumb": {
            background: "rgba(255, 255, 255, 0.2)",
            borderRadius: "3px",
          },
        }}
      >
        <VStack spacing={3} align="stretch">
          {/* Thinking section */}
          {showThinking && state.thinking && (
            <ThinkingSection
              thinking={state.thinking}
              isCollapsed={thinkingCollapsed}
              onToggle={() => setThinkingCollapsed(!thinkingCollapsed)}
            />
          )}

          {/* Tool calls */}
          {toolCallsArray.length > 0 && (
            <VStack spacing={2} align="stretch">
              {toolCallsArray.map((tool) => (
                <ToolCard key={tool.id} tool={tool} />
              ))}
            </VStack>
          )}

          {/* Main content */}
          {state.content && (
            <Box
              ref={contentRef}
              bg="gray.850"
              p={3}
              borderRadius="md"
              border="1px solid"
              borderColor="gray.700"
            >
              <Text
                fontSize="13px"
                color="gray.100"
                whiteSpace="pre-wrap"
                wordBreak="break-word"
                fontFamily="'Inter', -apple-system, sans-serif"
                lineHeight="1.7"
              >
                {state.content}
              </Text>
            </Box>
          )}

          {/* Error message */}
          {state.hasError && state.errorMessage && (
            <Box
              bg="red.900"
              p={3}
              borderRadius="md"
              border="1px solid"
              borderColor="red.600"
            >
              <HStack spacing={2} mb={2}>
                <AlertCircle size={14} color="#FC8181" />
                <Text fontSize="xs" color="red.200" fontWeight="bold">
                  Error
                </Text>
              </HStack>
              <Text fontSize="12px" color="red.100" whiteSpace="pre-wrap">
                {state.errorMessage}
              </Text>
            </Box>
          )}

          {/* Loading indicator when nothing yet */}
          {!state.thinking && !state.content && toolCallsArray.length === 0 && (
            <HStack spacing={3} justify="center" py={4}>
              <Spinner size="sm" color="blue.400" />
              <Text fontSize="sm" color="gray.400">
                Waiting for Claude...
              </Text>
            </HStack>
          )}
        </VStack>
      </Box>
    </Box>
  );
};

export default ClaudeStreamView;

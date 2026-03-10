import React, { useEffect, useState, useRef } from "react";
import {
  Box,
  VStack,
  HStack,
  Text,
  Collapse,
  IconButton,
  Badge,
  Tooltip,
} from "@chakra-ui/react";
import { ChevronDown, ChevronRight, Terminal, Copy, Check } from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import { ClaudeStreamView } from "./ClaudeStreamView";

interface TerminalLine {
  line: string;
  stream: "stdout" | "stderr";
  timestamp: number;
}

interface TerminalOutputProps {
  /** Process ID to filter output for */
  processId?: string;
  /** Whether to show all terminal output (if no processId) */
  showAll?: boolean;
  /** Initial collapsed state */
  defaultCollapsed?: boolean;
  /** Max height of the terminal output area */
  maxHeight?: string;
  /** Command that's running (for display) */
  command?: string;
  /** Whether to show thinking for Claude CLI (default: true) */
  showThinking?: boolean;
}

export const TerminalOutput: React.FC<TerminalOutputProps> = ({
  processId,
  showAll = false,
  defaultCollapsed = true,
  maxHeight = "300px",
  command,
  showThinking = true,
}) => {
  const [lines, setLines] = useState<TerminalLine[]>([]);
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);
  const [copied, setCopied] = useState(false);
  const [isClaudeCli, setIsClaudeCli] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Auto-expand when first output arrives
  useEffect(() => {
    if (lines.length === 1 && defaultCollapsed) {
      setIsCollapsed(false);
    }
  }, [lines.length, defaultCollapsed]);

  // Listen for Claude CLI detection
  useEffect(() => {
    if (!processId) return;

    const unlisten = listen<{
      process_id: string;
      command: string;
      timestamp: number;
    }>("claude-cli-detected", (event) => {
      if (event.payload.process_id === processId) {
        setIsClaudeCli(true);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [processId]);

  // Listen for terminal output events
  useEffect(() => {
    const unlisten = listen<{
      process_id: string;
      line: string;
      stream: "stdout" | "stderr";
      timestamp: number;
    }>("terminal-output", (event) => {
      const { process_id, line, stream, timestamp } = event.payload;

      // Filter by processId if specified
      if (processId && process_id !== processId) {
        return;
      }

      // Skip if not showing all and no processId specified
      if (!showAll && !processId) {
        return;
      }

      setLines((prev) => [...prev, { line, stream, timestamp }]);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [processId, showAll]);

  // Auto-scroll to bottom when new lines arrive
  useEffect(() => {
    if (scrollRef.current && !isCollapsed) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [lines, isCollapsed]);

  // Copy all output to clipboard
  const handleCopy = async () => {
    const text = lines.map((l) => l.line).join("\n");
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  // If this is a Claude CLI process, show the rich Claude view
  if (isClaudeCli && processId) {
    return (
      <Box mt={3}>
        <ClaudeStreamView
          processId={processId}
          showThinking={showThinking}
          maxHeight={maxHeight}
          command={command}
        />
      </Box>
    );
  }

  // Otherwise show regular terminal output
  if (lines.length === 0) {
    return null;
  }

  return (
    <Box
      mt={3}
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
        cursor="pointer"
        onClick={() => setIsCollapsed(!isCollapsed)}
        justify="space-between"
        _hover={{ bg: "gray.750" }}
      >
        <HStack spacing={2}>
          <IconButton
            aria-label="Toggle terminal output"
            icon={isCollapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            size="xs"
            variant="ghost"
            color="gray.400"
            _hover={{ bg: "transparent" }}
          />
          <Terminal size={14} color="#9ca3af" />
          <Text fontSize="xs" color="gray.400" fontFamily="mono">
            {command ? `$ ${command.slice(0, 40)}${command.length > 40 ? "..." : ""}` : "Terminal Output"}
          </Text>
          <Badge colorScheme="green" fontSize="10px" ml={2}>
            {lines.length} lines
          </Badge>
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

      {/* Terminal output */}
      <Collapse in={!isCollapsed} animateOpacity>
        <Box
          ref={scrollRef}
          maxH={maxHeight}
          overflowY="auto"
          p={3}
          fontFamily="'JetBrains Mono', 'Fira Code', 'SF Mono', Monaco, monospace"
          fontSize="12px"
          lineHeight="1.6"
          sx={{
            "&::-webkit-scrollbar": {
              width: "6px",
            },
            "&::-webkit-scrollbar-track": {
              background: "rgba(0, 0, 0, 0.2)",
            },
            "&::-webkit-scrollbar-thumb": {
              background: "rgba(255, 255, 255, 0.2)",
              borderRadius: "3px",
            },
            "&::-webkit-scrollbar-thumb:hover": {
              background: "rgba(255, 255, 255, 0.3)",
            },
          }}
        >
          <VStack spacing={0} align="stretch">
            {lines.map((item, index) => (
              <Text
                key={index}
                color={item.stream === "stderr" ? "red.400" : "green.300"}
                whiteSpace="pre-wrap"
                wordBreak="break-all"
              >
                {item.line}
              </Text>
            ))}
          </VStack>
        </Box>
      </Collapse>
    </Box>
  );
};

// Hook to track terminal process for a step
export const useTerminalProcess = () => {
  const [activeProcessId, setActiveProcessId] = useState<string | null>(null);
  const [activeCommand, setActiveCommand] = useState<string | null>(null);
  const [isClaudeCli, setIsClaudeCli] = useState(false);

  useEffect(() => {
    // Listen for terminal process started events
    const unlistenStart = listen<{
      process_id: string;
      command: string;
    }>("terminal-process-started", (event) => {
      setActiveProcessId(event.payload.process_id);
      setActiveCommand(event.payload.command);
      setIsClaudeCli(false); // Reset on new process
    });

    // Listen for Claude CLI detection
    const unlistenClaude = listen<{
      process_id: string;
      command: string;
    }>("claude-cli-detected", (event) => {
      if (event.payload.process_id === activeProcessId) {
        setIsClaudeCli(true);
      }
    });

    // Listen for terminal process completed events
    const unlistenEnd = listen<{
      process_id: string;
    }>("terminal-process-completed", (event) => {
      if (event.payload.process_id === activeProcessId) {
        // Keep the processId but mark as completed
        // This allows viewing the output after completion
      }
    });

    return () => {
      unlistenStart.then((fn) => fn());
      unlistenClaude.then((fn) => fn());
      unlistenEnd.then((fn) => fn());
    };
  }, [activeProcessId]);

  return { activeProcessId, activeCommand, isClaudeCli };
};

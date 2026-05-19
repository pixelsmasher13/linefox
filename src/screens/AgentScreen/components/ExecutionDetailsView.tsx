import React, { useEffect, useState, useRef, useMemo } from "react";
import {
  Box,
  VStack,
  HStack,
  Text as ChakraText,
  Badge,
  Flex,
  Button,
  Textarea,
  IconButton,
  Spinner,
  Tooltip,
  Collapse,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";
import { CheckCircle, AlertCircle, Clock, Square, Play, Send, Edit, ChevronDown, ChevronRight, CalendarClock } from "lucide-react";
import { ExecutionRunWithSteps } from "./ExecutionHistoryView";
import { ClipboardModal } from "./ClipboardModal";
import { invoke } from "@tauri-apps/api/core";
import { MarkdownContent } from "../../../components/MarkdownContent";

// Step with step_type field
interface ExecutionStep {
  id: number;
  execution_run_id: number;
  step_number: number;
  timestamp: string;
  explanation: string | null;
  next_step: string | null;
  status: string;
  error_message: string | null;
  created_at: string;
  step_type?: string; // 'action' | 'user_prompt' | 'completion'
}

// A conversation segment groups related steps together
interface ConversationSegment {
  type: 'initial' | 'continuation';
  userMessage: string; // The user's prompt (objective or continuation)
  actionSteps: ExecutionStep[]; // The action steps that follow
}

interface ExecutionDetailsViewProps {
  execution: ExecutionRunWithSteps;
  automationName?: string;
  onRerun?: (automationId: number, additionalInstructions?: string) => void;
  onContinue?: (automationId: number, continuationPrompt: string, previousContext: ContinuationContext) => Promise<void>;
  onEdit?: (automationId: number) => void;
  onSchedule?: (automationId: number, executionRunId: number) => void;
}

export interface ContinuationContext {
  previousCompletionMessage: string;
  previousObjective: string;
  recentSteps: string[];
  executionRunId: number;
}

export const ExecutionDetailsView: React.FC<ExecutionDetailsViewProps> = ({ execution, automationName, onRerun, onContinue, onEdit, onSchedule }) => {
  const [objective, setObjective] = useState<string>("");
  const [continuationPrompt, setContinuationPrompt] = useState("");
  const [isContinuing, setIsContinuing] = useState(false);
  const [expandedSegments, setExpandedSegments] = useState<Set<number>>(new Set());
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const fetchObjective = async () => {
      try {
        const automation = await invoke<any>("get_automation_by_id", { automationId: execution.run.automation_id });
        if (automation && automation.objective) {
          setObjective(automation.objective);
        }
      } catch (error) {
        console.error("Failed to fetch automation objective:", error);
      }
    };
    fetchObjective();
  }, [execution.run.automation_id]);

  // Organize steps into conversation segments
  const conversationSegments = useMemo((): ConversationSegment[] => {
    const segments: ConversationSegment[] = [];
    const steps = execution.steps as ExecutionStep[];
    
    // First segment is the initial objective
    let currentSegment: ConversationSegment = {
      type: 'initial',
      userMessage: objective,
      actionSteps: [],
    };
    
    for (const step of steps) {
      if (step.step_type === 'user_prompt') {
        // Save current segment if it has content
        if (currentSegment.userMessage || currentSegment.actionSteps.length > 0) {
          segments.push(currentSegment);
        }
        // Start a new segment for this continuation
        currentSegment = {
          type: 'continuation',
          userMessage: step.explanation || '',
          actionSteps: [],
        };
      } else {
        // Regular action step
        currentSegment.actionSteps.push(step);
      }
    }
    
    // Push the last segment
    if (currentSegment.userMessage || currentSegment.actionSteps.length > 0) {
      segments.push(currentSegment);
    }
    
    return segments;
  }, [execution.steps, objective]);

  const toggleSegment = (index: number) => {
    setExpandedSegments(prev => {
      const next = new Set(prev);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  };

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
      case "completed":
        return "green";
      case "running":
      case "executing":
        return "blue";
      case "failed":
      case "error":
        return "red";
      case "stopped":
        return "gray";
      default:
        return "gray";
    }
  };

  const formatDuration = (startedAt: string, completedAt?: string) => {
    const start = new Date(startedAt).getTime();
    const end = completedAt ? new Date(completedAt).getTime() : Date.now();
    const durationMs = end - start;
    
    const seconds = Math.floor(durationMs / 1000);
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    
    if (minutes > 0) {
      return `${minutes}m ${remainingSeconds}s`;
    }
    return `${seconds}s`;
  };

  const handleContinue = async () => {
    if (!continuationPrompt.trim() || !onContinue || isContinuing) return;
    
    setIsContinuing(true);
    try {
      const recentSteps = execution.steps
        .slice(-20)
        .map(step => {
          let stepDesc = `Step ${step.step_number + 1}`;
          if (step.explanation) stepDesc += `: ${step.explanation}`;
          if (step.next_step) stepDesc += ` → ${step.next_step}`;
          return stepDesc;
        });

      const context: ContinuationContext = {
        previousCompletionMessage: execution.run.completion_message || "",
        previousObjective: objective,
        recentSteps,
        executionRunId: execution.run.id,
      };

      await onContinue(execution.run.automation_id, continuationPrompt, context);
      setContinuationPrompt("");
    } finally {
      setIsContinuing(false);
    }
  };

  const canContinue = execution.run.status === "completed" || execution.run.status === "stopped";

  return (
    <Box width="100%" height="100%" overflow="hidden" display="flex" flexDirection="column">
      {/* Header with Play button on top right */}
      <Box px={6} py={4} borderBottom="1px solid" borderColor="gray.200" bg="white">
        <HStack justify="space-between" align="center">
          <HStack spacing={3}>
            {getStatusIcon(execution.run.status)}
            <Text type="l" bold>
              {automationName || 'Task Details'}
            </Text>
            <Badge colorScheme={getStatusColor(execution.run.status)} size="sm">
              {execution.run.status}
            </Badge>
            {execution.run.completed_at && (
              <ChakraText fontSize="xs" color="gray.500">
                {formatDuration(execution.run.started_at, execution.run.completed_at)}
              </ChakraText>
            )}
          </HStack>
          
          <HStack spacing={2}>
            {execution.run.clipboard && (
              <ClipboardModal
                clipboardContent={execution.run.clipboard}
                onClose={() => {}}
              />
            )}
            {onEdit && (
              <Tooltip label="Edit task plan">
                <Button
                  size="sm"
                  leftIcon={<Edit size={14} />}
                  variant="outline"
                  onClick={() => onEdit(execution.run.automation_id)}
                >
                  Edit
                </Button>
              </Tooltip>
            )}
            {onSchedule && (execution.run.status === 'completed' || execution.run.status === 'stopped') && (
              <Tooltip label="Run on a schedule">
                <Button
                  size="sm"
                  leftIcon={<CalendarClock size={14} />}
                  variant="outline"
                  onClick={() => onSchedule(execution.run.automation_id, execution.run.id)}
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
                  onClick={() => onRerun(execution.run.automation_id, execution.run.additional_instructions)}
                >
                  Run Again
                </Button>
              </Tooltip>
            )}
          </HStack>
        </HStack>
      </Box>
      
      {/* Main scrollable content */}
      <Box
        flex={1}
        overflow="auto"
        p={6}
        bg="gray.50"
        sx={{
          '&::-webkit-scrollbar': { width: '6px' },
          '&::-webkit-scrollbar-track': { background: 'rgba(0, 0, 0, 0.05)' },
          '&::-webkit-scrollbar-thumb': { background: 'rgba(0, 0, 0, 0.2)', borderRadius: '5px' },
        }}
      >
        <VStack spacing={4} align="stretch" maxW="800px" mx="auto">
          {/* Render conversation segments (objective bubbles + collapsed steps) */}
          {conversationSegments.map((segment, segmentIndex) => (
            <Box key={segmentIndex}>
              {/* User message bubble */}
              {segment.userMessage && (
                <Flex justify="flex-end" mb={3}>
                  <Box
                    bg="blue.500"
                    color="white"
                    px={4}
                    py={3}
                    borderRadius="xl"
                    borderTopRightRadius="sm"
                    maxW="85%"
                  >
                    <ChakraText fontSize="sm" whiteSpace="pre-wrap" lineHeight="1.5">
                      {segment.userMessage}
                    </ChakraText>
                  </Box>
                </Flex>
              )}

              {/* Collapsible action steps */}
              {segment.actionSteps.length > 0 && (
                <Box mb={3}>
                  <HStack
                    spacing={2}
                    cursor="pointer"
                    onClick={() => toggleSegment(segmentIndex)}
                    py={2}
                    _hover={{ bg: 'gray.100' }}
                    borderRadius="md"
                    px={2}
                  >
                    {expandedSegments.has(segmentIndex) ? (
                      <ChevronDown size={16} color="#6b7280" />
                    ) : (
                      <ChevronRight size={16} color="#6b7280" />
                    )}
                    <ChakraText fontSize="xs" fontWeight="semibold" color="gray.500" textTransform="uppercase" letterSpacing="wide">
                      {segment.actionSteps.length} steps executed
                    </ChakraText>
                  </HStack>

                  <Collapse in={expandedSegments.has(segmentIndex)} animateOpacity>
                    <VStack spacing={2} align="stretch" mt={2} pl={6}>
                      {segment.actionSteps.map((step) => (
                        <Box
                          key={step.id}
                          bg="white"
                          px={4}
                          py={3}
                          borderRadius="md"
                          borderLeft="3px solid"
                          borderLeftColor={
                            step.status === "completed" ? "green.400" :
                            step.status === "error" ? "red.400" : "blue.400"
                          }
                        >
                          <HStack spacing={2} align="start">
                            <ChakraText fontSize="xs" color="gray.400" minW="40px">
                              #{step.step_number + 1}
                            </ChakraText>
                            <ChakraText fontSize="sm" color="gray.700" flex={1}>
                              {step.explanation || step.next_step || 'Executing...'}
                            </ChakraText>
                          </HStack>
                          {step.error_message && (
                            <ChakraText fontSize="xs" color="red.500" mt={1} ml="48px">
                              {step.error_message}
                            </ChakraText>
                          )}
                        </Box>
                      ))}
                    </VStack>
                  </Collapse>
                </Box>
              )}
            </Box>
          ))}

          {/* Completion summary — after conversation flow */}
          {execution.run.completion_message && execution.run.status === "completed" && (
            <Box
              bg="white"
              padding={5}
              borderRadius="lg"
              border="1px solid"
              borderColor="gray.200"
              boxShadow="sm"
            >
              <MarkdownContent content={execution.run.completion_message!} />
            </Box>
          )}

          {/* Status message for stopped/interrupted executions */}
          {execution.run.status === 'stopped' && !execution.run.error_message && (
            <Box bg="gray.50" p={4} borderRadius="lg" border="1px solid" borderColor="gray.200">
              <HStack spacing={2} align="start">
                <Square size={16} color="#9ca3af" />
                <ChakraText fontSize="sm" color="gray.600">
                  Task stopped by user
                </ChakraText>
              </HStack>
            </Box>
          )}

          {/* Error message if any (for actual errors, not user stops) */}
          {execution.run.error_message && (
            <Box 
              bg={execution.run.status === 'interrupted' ? "gray.50" : "red.50"} 
              p={4} 
              borderRadius="lg" 
              border="1px solid" 
              borderColor={execution.run.status === 'interrupted' ? "gray.200" : "red.200"}
            >
              <HStack spacing={2} align="start">
                <AlertCircle size={16} color={execution.run.status === 'interrupted' ? "#9ca3af" : "#ef4444"} />
                <ChakraText fontSize="sm" color={execution.run.status === 'interrupted' ? "gray.600" : "red.700"}>
                  {execution.run.error_message}
                </ChakraText>
              </HStack>
            </Box>
          )}
        </VStack>
      </Box>
      
      {/* Continuation input - larger and more prominent */}
      {canContinue && onContinue && (
        <Box
          p={5}
          borderTop="1px solid"
          borderColor="gray.200"
          bg="white"
        >
          <VStack spacing={3} maxW="800px" mx="auto">
            <HStack spacing={3} align="end" width="100%">
              <Textarea
                ref={textareaRef}
                placeholder="Continue this task... (e.g., 'Now also export this to PDF' or 'Find their email addresses too')"
                value={continuationPrompt}
                onChange={(e) => {
                  setContinuationPrompt(e.target.value);
                  e.target.style.height = 'auto';
                  e.target.style.height = `${Math.min(e.target.scrollHeight, 150)}px`;
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    handleContinue();
                  }
                }}
                rows={2}
                resize="none"
                isDisabled={isContinuing}
                fontSize="md"
                bg="gray.50"
                border="2px solid"
                borderColor="gray.200"
                borderRadius="xl"
                _hover={{ borderColor: 'gray.300' }}
                _focus={{ borderColor: 'blue.400', boxShadow: '0 0 0 1px var(--chakra-colors-blue-400)', bg: 'white' }}
                minH="80px"
                maxH="150px"
                p={4}
              />
              <IconButton
                aria-label="Continue task"
                icon={isContinuing ? <Spinner size="sm" /> : <Send size={20} />}
                colorScheme="blue"
                onClick={handleContinue}
                isDisabled={!continuationPrompt.trim() || isContinuing}
                size="lg"
                borderRadius="xl"
                h="80px"
                w="60px"
              />
            </HStack>
          </VStack>
        </Box>
      )}
    </Box>
  );
};

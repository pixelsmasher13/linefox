import React, { useEffect, useState, useRef } from "react";
import {
  Box,
  VStack,
  HStack,
  Button,
  Text as ChakraText,
  Spinner,
  Badge,
  Flex,
  useToast,
  Textarea,
  IconButton,
  Collapse,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  ModalCloseButton,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";
import { Square, CheckCircle, AlertCircle, Clock, Send, ChevronDown, ChevronRight, Copy, Database, FileText, Columns2, Rows2 } from "lucide-react";
import { listen, emit } from "@tauri-apps/api/event";
import { ClipboardModal } from "./ClipboardModal";
import { ContinuationContext } from "./ExecutionDetailsView";
import { TerminalOutput } from "../../../components/TerminalOutput";
import { MarkdownContent } from "../../../components/MarkdownContent";

interface AutomationStep {
  id: string;
  timestamp: string;
  explanation: string;
  nextStep?: string;
  status: "pending" | "executing" | "completed" | "error";
  error?: string;
  terminalProcessId?: string;
  terminalCommand?: string;
}

interface AgentPhase {
  phaseName: string;
  phaseNumber: number;
  goal: string;
  steps: string[];
  nextPhaseHint?: string;
}

interface AutomationExecutionViewProps {
  automationId: number;
  automationName: string;
  onStop: () => void;
  isPlaying: boolean;
  onClose?: () => void;
  onContinue?: (automationId: number, continuationPrompt: string, previousContext: ContinuationContext) => Promise<void>;
  executionRunId?: number;
}

export const AutomationExecutionView: React.FC<AutomationExecutionViewProps> = ({
  automationId,
  automationName,
  onStop,
  isPlaying,
  onClose,
  onContinue,
  executionRunId,
}) => {
  const [steps, setSteps] = useState<AutomationStep[]>([]);
  const [currentStepIndex, setCurrentStepIndex] = useState(-1);
  const [completionMessage, setCompletionMessage] = useState<string | null>(null);
  const [objective, setObjective] = useState<string>("");
  const [clipboardContent, setClipboardContent] = useState<string | null>(null);
  const [continuationPrompt, setContinuationPrompt] = useState("");
  const [isContinuing, setIsContinuing] = useState(false);
  const [userMessage, setUserMessage] = useState("");
  const [currentRunId, setCurrentRunId] = useState<number | undefined>(executionRunId);
  const [currentPhase, setCurrentPhase] = useState<AgentPhase | null>(null);
  const [completedPhases, setCompletedPhases] = useState<AgentPhase[]>([]);
  const [isPhaseCardCollapsed, setIsPhaseCardCollapsed] = useState(false);
  const [splitLayout, setSplitLayout] = useState<boolean>(() => {
    if (typeof window !== "undefined") {
      return window.localStorage.getItem("linefox.execution.splitLayout") === "true";
    }
    return false;
  });

  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem("linefox.execution.splitLayout", String(splitLayout));
    }
    if (splitLayout && isPhaseCardCollapsed) {
      setIsPhaseCardCollapsed(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [splitLayout]);
  const [isStepsCollapsed, setIsStepsCollapsed] = useState(false);
  const [isClipboardModalOpen, setIsClipboardModalOpen] = useState(false);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const toast = useToast();

  // Fetch objective when component mounts
  useEffect(() => {
    const fetchObjective = async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const automation = await invoke<any>("get_automation_by_id", { automationId });
        if (automation && automation.objective) {
          setObjective(automation.objective);
        }
      } catch (error) {
        console.error("Failed to fetch automation objective:", error);
      }
    };
    fetchObjective();
  }, [automationId]);

  useEffect(() => {
    // Clear steps and completion message when automation starts
    if (isPlaying) {
      setSteps([]);
      setCurrentStepIndex(-1);
      setCompletionMessage(null);
      setClipboardContent(null);
      setContinuationPrompt("");
      setCurrentPhase(null);
      setCompletedPhases([]);
      setIsPhaseCardCollapsed(false);
    }
  }, [isPlaying, automationId]);
  
  // Sync currentRunId from prop when parent provides it
  useEffect(() => {
    if (executionRunId !== undefined) {
      setCurrentRunId(executionRunId);
    }
  }, [executionRunId]);

  // Listen for automation_started to capture run ID (fallback if prop not available)
  useEffect(() => {
    const unsubscribe = listen("automation_started", (event: any) => {
      if (event.payload && event.payload.execution_run_id) {
        setCurrentRunId(event.payload.execution_run_id);
      }
    });
    
    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, []);

  // Listen for agent phase updates (orchestrator plan)
  useEffect(() => {
    if (!isPlaying) return;

    const unsubscribe = listen("agent_phase_started", (event: any) => {
      const { phaseName, phaseNumber, goal, steps, nextPhaseHint } = event.payload;
      console.log("Agent phase started:", phaseName, "with", steps?.length, "steps");

      setCurrentPhase((prev) => {
        if (prev) {
          setCompletedPhases((list) => [...list, prev]);
        }
        return {
          phaseName,
          phaseNumber,
          goal,
          steps: steps || [],
          nextPhaseHint,
        };
      });
      setIsPhaseCardCollapsed(false);
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, [isPlaying]);

  // Auto-collapse the phase card after the first execution step arrives,
  // so the running task stays visible as steps accumulate.
  useEffect(() => {
    if (currentPhase && steps.length >= 1 && !isPhaseCardCollapsed && !splitLayout) {
      setIsPhaseCardCollapsed(true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentPhase, steps.length >= 1, splitLayout]);

  useEffect(() => {
    if (!isPlaying) return;

    // Listen for automation step updates
    const unsubscribe = listen("automation_step", (event: any) => {
      const { automationId: eventAutomationId, explanation, nextStep, status, error } = event.payload;

      if (eventAutomationId !== automationId) return;

      const newStep: AutomationStep = {
        id: `step-${Date.now()}`,
        timestamp: new Date().toLocaleTimeString(),
        explanation,
        nextStep,
        status: status || "executing",
        error,
      };

      setSteps((prevSteps) => {
        // Always add as a new step since backend sends complete step info
        const updatedSteps = [...prevSteps];

        // Mark previous step as completed if it exists and we have a new explanation
        if (explanation && updatedSteps.length > 0) {
          updatedSteps[updatedSteps.length - 1].status = "completed";
        }

        // Add the new step
        updatedSteps.push(newStep);
        setCurrentStepIndex(updatedSteps.length - 1);

        return updatedSteps;
      });
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, [isPlaying, automationId]);

  // Listen for terminal process started events to attach to current step
  useEffect(() => {
    const unsubscribe = listen("terminal-process-started", (event: any) => {
      const { process_id, command } = event.payload;
      
      // Attach terminal process to the current executing step
      setSteps((prevSteps) => {
        if (prevSteps.length === 0) return prevSteps;
        
        const updatedSteps = [...prevSteps];
        const lastStep = updatedSteps[updatedSteps.length - 1];
        
        // Only attach if the step is executing
        if (lastStep.status === "executing") {
          updatedSteps[updatedSteps.length - 1] = {
            ...lastStep,
            terminalProcessId: process_id,
            terminalCommand: command,
          };
        }
        
        return updatedSteps;
      });
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, []);

  // Listen for completion message
  useEffect(() => {
    const unsubscribe = listen("automation_completion_message", (event: any) => {
      const { message } = event.payload;
      console.log("Received completion message:", message);
      setCompletionMessage(message);
      setIsStepsCollapsed(true);

      // Mark the last step as completed
      setSteps((prevSteps) => {
        if (prevSteps.length > 0) {
          const updatedSteps = [...prevSteps];
          updatedSteps[updatedSteps.length - 1].status = "completed";
          return updatedSteps;
        }
        return prevSteps;
      });
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, []);

  // Listen for automation stopped event
  useEffect(() => {
    const unsubscribe = listen("automation_stopped", (event: any) => {
      console.log("Automation stopped, marking last step as completed");

      if (event.payload && event.payload.clipboard) {
        setClipboardContent(event.payload.clipboard);
      }

      // Mark the last step as completed
      setSteps((prevSteps) => {
        if (prevSteps.length > 0) {
          const updatedSteps = [...prevSteps];
          updatedSteps[updatedSteps.length - 1].status = "completed";
          return updatedSteps;
        }
        return prevSteps;
      });

      // Scroll to top when automation completes
      if (scrollContainerRef.current) {
        scrollContainerRef.current.scrollTo({ top: 0, behavior: 'smooth' });
      }
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, []);

  // Scroll to top when completion message appears
  useEffect(() => {
    if (completionMessage && scrollContainerRef.current) {
      // Small timeout to ensure rendering is complete
      setTimeout(() => {
        if (scrollContainerRef.current) {
          scrollContainerRef.current.scrollTop = 0;
        }
      }, 50);
    }
  }, [completionMessage]);

  // Auto-scroll to bottom when new steps are added
  useEffect(() => {
    // If we have a completion message, we want to stay at the top
    if (completionMessage) return;

    if (scrollContainerRef.current) {
      scrollContainerRef.current.scrollTop = scrollContainerRef.current.scrollHeight;
    }
  }, [steps, completionMessage]);

  const getStepIcon = (status: AutomationStep["status"]) => {
    switch (status) {
      case "completed":
        return <CheckCircle size={20} color="green" />;
      case "executing":
        return <Spinner size="sm" />;
      case "error":
        return <AlertCircle size={20} color="red" />;
      default:
        return <Clock size={20} color="gray" />;
    }
  };

  const getStepBadgeColor = (status: AutomationStep["status"]) => {
    switch (status) {
      case "completed":
        return "green";
      case "executing":
        return "blue";
      case "error":
        return "red";
      default:
        return "gray";
    }
  };

  const formatDuration = () => {
    if (steps.length < 2) return "";
    const first = steps[0].timestamp;
    const last = steps[steps.length - 1].timestamp;
    // Timestamps are from toLocaleTimeString(), parse them
    const parseTime = (t: string) => {
      const d = new Date();
      const parts = t.match(/(\d+):(\d+):(\d+)\s*(AM|PM)?/i);
      if (!parts) return d.getTime();
      let hours = parseInt(parts[1]);
      const mins = parseInt(parts[2]);
      const secs = parseInt(parts[3]);
      if (parts[4]?.toUpperCase() === 'PM' && hours !== 12) hours += 12;
      if (parts[4]?.toUpperCase() === 'AM' && hours === 12) hours = 0;
      d.setHours(hours, mins, secs, 0);
      return d.getTime();
    };
    const ms = parseTime(last) - parseTime(first);
    if (ms <= 0) return "";
    const totalSec = Math.floor(ms / 1000);
    const min = Math.floor(totalSec / 60);
    const sec = totalSec % 60;
    if (min > 0) return `${min}m ${sec}s`;
    return `${sec}s`;
  };

  const handleContinue = async () => {
    if (!continuationPrompt.trim() || !onContinue || !currentRunId) return;
    
    setIsContinuing(true);
    try {
      // Build recent steps summary
      const recentSteps = steps.slice(-20).map(step => 
        `[${step.timestamp}] ${step.explanation || 'Step executed'}${step.error ? ` - Error: ${step.error}` : ''}`
      );
      
      const context: ContinuationContext = {
        previousCompletionMessage: completionMessage || "",
        previousObjective: objective,
        recentSteps,
        executionRunId: currentRunId,
      };
      
      await onContinue(automationId, continuationPrompt, context);
      setContinuationPrompt("");
    } catch (error) {
      console.error("Failed to continue task:", error);
      toast({
        title: "Failed to continue task",
        description: String(error),
        status: "error",
        duration: 3000,
      });
    } finally {
      setIsContinuing(false);
    }
  };

  return (
    <VStack spacing={0} height="100%" width="100%">
      {/* Header */}
      <Box
        width="100%"
        padding={4}
        borderBottom="1px solid"
        borderColor="gray.200"
        bg="white"
      >
        <HStack justify="space-between">
          <VStack align="start" spacing={1}>
            <Text type="l" bold>
              {completionMessage ? automationName : `Executing: ${automationName}`}
            </Text>
            <Text type="s" secondary>
              {completionMessage ? "Completed successfully" : "Running agent..."}
            </Text>
          </VStack>
          <HStack spacing={2}>
            {clipboardContent && (
              <ClipboardModal
                clipboardContent={clipboardContent}
                onClose={() => {}}
              />
            )}
            <Badge colorScheme={completionMessage ? "green" : "blue"} fontSize="sm" px={3} py={1}>
              {completionMessage
                ? (formatDuration() ? `Completed in ${formatDuration()}` : "Completed")
                : `${steps.length} ${steps.length === 1 ? "Step" : "Steps"}`}
            </Badge>
          </HStack>
        </HStack>
      </Box>

      {/* Steps Container */}
      <Box
        ref={scrollContainerRef}
        flex={1}
        width="100%"
        overflowY="auto"
        bg="gray.50"
        padding={4}
        sx={{
          '&::-webkit-scrollbar': {
            width: '6px',
          },
          '&::-webkit-scrollbar-track': {
            background: 'rgba(0, 0, 0, 0.05)',
          },
          '&::-webkit-scrollbar-thumb': {
            background: 'rgba(0, 0, 0, 0.2)',
            borderRadius: '5px',
          },
          '&::-webkit-scrollbar-thumb:hover': {
            background: 'rgba(0, 0, 0, 0.3)',
          },
        }}
      >
        <VStack spacing={4} align="stretch">
          {/* === COMPLETED: Results-first layout === */}
          {completionMessage ? (
            <>
              {/* Completion summary — hero section */}
              <Box
                bg="white"
                padding={5}
                borderRadius="lg"
                border="1px solid"
                borderColor="gray.200"
                boxShadow="sm"
              >
                <MarkdownContent content={completionMessage} />
              </Box>

              {/* Collected data — inline preview + modal */}
              {clipboardContent && (
                <Box>
                  <HStack
                    spacing={2}
                    py={2}
                    px={2}
                    borderRadius="md"
                    cursor="pointer"
                    _hover={{ bg: "gray.50" }}
                    onClick={() => setIsClipboardModalOpen(true)}
                  >
                    <Database size={14} color="#6b7280" />
                    <ChakraText fontSize="xs" fontWeight="semibold" color="gray.500">
                      Collected Data
                    </ChakraText>
                  </HStack>

                  {/* Full modal */}
                  <Modal isOpen={isClipboardModalOpen} onClose={() => setIsClipboardModalOpen(false)} size="xl">
                    <ModalOverlay />
                    <ModalContent>
                      <ModalHeader>
                        <HStack spacing={2}>
                          <FileText size={20} />
                          <ChakraText fontWeight="bold">Collected Data</ChakraText>
                        </HStack>
                      </ModalHeader>
                      <ModalCloseButton />
                      <ModalBody>
                        <Box
                          p={4}
                          bg="gray.50"
                          borderRadius="md"
                          maxH="400px"
                          overflowY="auto"
                        >
                          <ChakraText fontSize="sm" whiteSpace="pre-wrap" fontFamily="mono">
                            {clipboardContent}
                          </ChakraText>
                        </Box>
                      </ModalBody>
                      <ModalFooter>
                        <Button variant="ghost" mr={3} onClick={() => setIsClipboardModalOpen(false)}>
                          Close
                        </Button>
                        <Button
                          colorScheme="blue"
                          leftIcon={<Copy size={16} />}
                          onClick={() => {
                            navigator.clipboard.writeText(clipboardContent);
                            toast({ title: "Copied to clipboard", status: "success", duration: 1500 });
                          }}
                        >
                          Copy to Clipboard
                        </Button>
                      </ModalFooter>
                    </ModalContent>
                  </Modal>
                </Box>
              )}

              {/* Collapsed step log */}
              {steps.length > 0 && (
                <Box>
                  <HStack
                    spacing={2}
                    cursor="pointer"
                    onClick={() => setIsStepsCollapsed(!isStepsCollapsed)}
                    py={2}
                    px={2}
                    _hover={{ bg: 'gray.100' }}
                    borderRadius="md"
                  >
                    {!isStepsCollapsed ? (
                      <ChevronDown size={16} color="#6b7280" />
                    ) : (
                      <ChevronRight size={16} color="#6b7280" />
                    )}
                    <ChakraText fontSize="xs" fontWeight="semibold" color="gray.500" textTransform="uppercase" letterSpacing="wide">
                      {steps.length} {steps.length === 1 ? "step" : "steps"} executed
                    </ChakraText>
                  </HStack>
                  <Collapse in={!isStepsCollapsed} animateOpacity>
                    <VStack spacing={2} align="stretch" mt={2} pl={6}>
                      {steps.map((step) => (
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
                              {step.timestamp}
                            </ChakraText>
                            <ChakraText fontSize="sm" color="gray.700" flex={1}>
                              {step.explanation || 'Step executed'}
                            </ChakraText>
                          </HStack>
                          {step.error && (
                            <ChakraText fontSize="xs" color="red.500" mt={1} ml="48px">
                              {step.error}
                            </ChakraText>
                          )}
                        </Box>
                      ))}
                    </VStack>
                  </Collapse>
                </Box>
              )}
            </>
          ) : (
            <>
              {/* === RUNNING: Live step-by-step view === */}

              {/* Layout toggle */}
              <Flex justify="flex-end" mb={3}>
                <Button
                  size="sm"
                  variant="outline"
                  borderColor={splitLayout ? "purple.300" : "gray.300"}
                  bg={splitLayout ? "purple.50" : "white"}
                  color={splitLayout ? "purple.700" : "gray.700"}
                  fontWeight="medium"
                  leftIcon={splitLayout ? <Rows2 size={14} /> : <Columns2 size={14} />}
                  onClick={() => setSplitLayout((s) => !s)}
                  _hover={{ bg: splitLayout ? "purple.100" : "gray.50" }}
                >
                  {splitLayout ? "Stacked view" : "Split view"}
                </Button>
              </Flex>

              <Flex
                direction={splitLayout ? { base: "column", md: "row" } : "column"}
                gap={splitLayout ? 4 : 0}
                align="stretch"
              >
                <Box flex={splitLayout ? "0 0 42%" : "auto"} minW={0}>

              {/* Completed phases — collapsed summary rows */}
              {completedPhases.length > 0 && (
                <VStack align="stretch" spacing={1.5} mb={3}>
                  {completedPhases.map((p) => (
                    <Flex
                      key={`${p.phaseNumber}-${p.phaseName}`}
                      align="center"
                      gap={2.5}
                      px={3}
                      py={2}
                      bg="gray.50"
                      border="1px solid"
                      borderColor="gray.200"
                      borderRadius="md"
                    >
                      <Flex
                        w="16px"
                        h="16px"
                        bg="green.500"
                        color="white"
                        borderRadius="full"
                        align="center"
                        justify="center"
                        flexShrink={0}
                      >
                        <CheckCircle size={10} strokeWidth={3} />
                      </Flex>
                      <Flex
                        bg="purple.100"
                        color="purple.700"
                        px={2}
                        py={0.5}
                        borderRadius="sm"
                        fontSize="2xs"
                        fontWeight="bold"
                        letterSpacing="wider"
                        flexShrink={0}
                      >
                        {`PHASE ${p.phaseNumber}`}
                      </Flex>
                      <ChakraText fontSize="sm" fontWeight="medium" color="gray.800" noOfLines={1} flex={1}>
                        {p.phaseName}
                      </ChakraText>
                      <ChakraText fontSize="xs" color="gray.500" flexShrink={0}>
                        {`${p.steps.length} steps`}
                      </ChakraText>
                    </Flex>
                  ))}
                </VStack>
              )}

              {/* Current Phase Plan (Agent Mode) — collapses once steps start streaming */}
              {currentPhase && (
                isPhaseCardCollapsed ? (
                  <Flex
                    as="button"
                    onClick={() => setIsPhaseCardCollapsed(false)}
                    bg="white"
                    border="1px solid"
                    borderColor="gray.200"
                    borderLeft="3px solid"
                    borderLeftColor="purple.500"
                    borderRadius="md"
                    px={4}
                    py={2.5}
                    mb={4}
                    align="center"
                    gap={2.5}
                    w="full"
                    cursor="pointer"
                    _hover={{ bg: "gray.50" }}
                    boxShadow="sm"
                  >
                    <Flex
                      bg="purple.600"
                      color="white"
                      px={2}
                      py={0.5}
                      borderRadius="sm"
                      fontSize="xs"
                      fontWeight="bold"
                      letterSpacing="wider"
                      flexShrink={0}
                    >
                      {`PHASE ${currentPhase.phaseNumber}`}
                    </Flex>
                    <ChakraText fontSize="sm" fontWeight="semibold" color="gray.900" noOfLines={1} flex={1} textAlign="left">
                      {currentPhase.phaseName}
                    </ChakraText>
                    <ChakraText fontSize="xs" color="gray.500" flexShrink={0}>
                      {`Step ${Math.max(currentStepIndex + 1, 1)} of ${currentPhase.steps.length || steps.length}`}
                    </ChakraText>
                    <Box color="gray.400" flexShrink={0}>
                      <ChevronRight size={16} />
                    </Box>
                  </Flex>
                ) : (
                  <Box
                    bg="white"
                    padding={5}
                    borderRadius="md"
                    border="1px solid"
                    borderColor="gray.200"
                    borderLeft="3px solid"
                    borderLeftColor="purple.500"
                    boxShadow="sm"
                    mb={4}
                    position="relative"
                  >
                    {steps.length > 0 && (
                      <Box
                        as="button"
                        onClick={() => setIsPhaseCardCollapsed(true)}
                        position="absolute"
                        top={3}
                        right={3}
                        color="gray.400"
                        _hover={{ color: "gray.700" }}
                        aria-label="Collapse plan"
                      >
                        <ChevronDown size={16} />
                      </Box>
                    )}
                    <VStack align="start" spacing={3.5}>
                      <HStack spacing={2.5}>
                        <Flex
                          bg="purple.600"
                          color="white"
                          px={2}
                          py={0.5}
                          borderRadius="sm"
                          fontSize="xs"
                          fontWeight="bold"
                          letterSpacing="wider"
                        >
                          {`PHASE ${currentPhase.phaseNumber}`}
                        </Flex>
                        <ChakraText fontSize="lg" fontWeight="semibold" color="gray.900">
                          {currentPhase.phaseName}
                        </ChakraText>
                      </HStack>

                      {currentPhase.goal && (
                        <Box>
                          <ChakraText fontSize="xs" fontWeight="bold" color="gray.500" textTransform="uppercase" letterSpacing="wider" mb={1}>
                            Goal
                          </ChakraText>
                          <ChakraText fontSize="md" color="gray.800" lineHeight="1.55">
                            {currentPhase.goal}
                          </ChakraText>
                        </Box>
                      )}

                      {currentPhase.steps.length > 0 && (
                        <Box width="100%">
                          <ChakraText fontSize="xs" fontWeight="bold" color="gray.500" textTransform="uppercase" letterSpacing="wider" mb={2}>
                            {`Plan · ${currentPhase.steps.length} steps`}
                          </ChakraText>
                          <VStack align="stretch" spacing={1.5}>
                            {currentPhase.steps.map((step, idx) => (
                              <HStack key={idx} align="flex-start" spacing={2.5}>
                                <ChakraText fontSize="sm" color="gray.400" fontFamily="mono" lineHeight="1.55" flexShrink={0} minW="20px" fontWeight="semibold">
                                  {idx + 1}.
                                </ChakraText>
                                <ChakraText fontSize="sm" color="gray.800" lineHeight="1.55">
                                  {step}
                                </ChakraText>
                              </HStack>
                            ))}
                          </VStack>
                        </Box>
                      )}

                      {currentPhase.nextPhaseHint && (
                        <ChakraText fontSize="sm" color="gray.500" fontStyle="italic">
                          Next: {currentPhase.nextPhaseHint}
                        </ChakraText>
                      )}
                    </VStack>
                  </Box>
                )
              )}

                </Box>
                <Box flex={splitLayout ? "1 1 58%" : "auto"} minW={0}>

              {steps.length === 0 ? (
                <Flex justify="center" align="center" height="200px">
                  <VStack spacing={4}>
                    <Spinner size="lg" color="blue.500" />
                    <Text type="m" secondary>
                      {currentPhase ? "Starting phase execution..." : "Initializing the agent..."}
                    </Text>
                  </VStack>
                </Flex>
              ) : (
                steps.map((step, index) => (
                  <Box
                    key={step.id}
                    bg="white"
                    padding={4}
                    borderRadius="md"
                    border="1px solid"
                    borderColor={
                      index === currentStepIndex && step.status === "executing"
                        ? "blue.400"
                        : "gray.200"
                    }
                    boxShadow={
                      index === currentStepIndex && step.status === "executing"
                        ? "0 0 0 3px rgba(66, 153, 225, 0.1)"
                        : "sm"
                    }
                  >
                    <HStack align="start" spacing={3}>
                      <Box pt={1}>{getStepIcon(step.status)}</Box>
                      <VStack align="start" spacing={2} flex={1}>
                        <HStack>
                          <Text type="s" secondary>
                            {step.timestamp}
                          </Text>
                          <Badge colorScheme={getStepBadgeColor(step.status)} size="sm">
                            {step.status}
                          </Badge>
                        </HStack>

                        {step.explanation && (
                          <Box>
                            <Box mb={1}>
                              <Text type="s" bold>
                                What I'm doing:
                              </Text>
                            </Box>
                            <ChakraText fontSize="sm" color="gray.700">
                              {step.explanation}
                            </ChakraText>
                          </Box>
                        )}

                        {step.nextStep && step.status === "executing" && (
                          <Box
                            bg="blue.50"
                            p={3}
                            borderRadius="sm"
                            borderLeft="3px solid"
                            borderColor="blue.400"
                            width="100%"
                          >
                            <Box mb={1}>
                              <ChakraText fontSize="sm" fontWeight="bold" color="blue.700">
                                Next Step:
                              </ChakraText>
                            </Box>
                            <ChakraText fontSize="sm" color="blue.700">
                              {step.nextStep}
                            </ChakraText>
                          </Box>
                        )}

                        {step.error && (
                          <Box
                            bg="red.50"
                            p={3}
                            borderRadius="sm"
                            borderLeft="3px solid"
                            borderColor="red.400"
                            width="100%"
                          >
                            <Box mb={1}>
                              <ChakraText fontSize="sm" fontWeight="bold" color="red.700">
                                Error:
                              </ChakraText>
                            </Box>
                            <ChakraText fontSize="sm" color="red.700">
                              {step.error}
                            </ChakraText>
                          </Box>
                        )}

                        {/* Terminal output for this step */}
                        {step.terminalProcessId && (
                          <Box width="100%">
                            <TerminalOutput
                              processId={step.terminalProcessId}
                              command={step.terminalCommand}
                              defaultCollapsed={step.status === "completed"}
                              maxHeight="250px"
                            />
                          </Box>
                        )}
                      </VStack>
                    </HStack>
                  </Box>
                ))
              )}

                </Box>
              </Flex>
            </>
          )}
        </VStack>
      </Box>

      {/* Footer with Stop Button or Continuation Input */}
      <Box
        width="100%"
        padding={4}
        borderTop="1px solid"
        borderColor="gray.200"
        bg="white"
      >
        {isPlaying ? (
          <HStack spacing={2} width="100%">
            <Textarea
              placeholder="Send a message to the agent..."
              value={userMessage}
              onChange={(e) => setUserMessage(e.target.value)}
              onKeyDown={async (e) => {
                if (e.key === 'Enter' && !e.shiftKey && userMessage.trim()) {
                  e.preventDefault();
                  try {
                    const { invoke } = await import("@tauri-apps/api/core");
                    await invoke("send_user_message", { message: userMessage.trim() });
                    toast({ title: "Message sent", status: "success", duration: 2000 });
                    setUserMessage("");
                  } catch (err) {
                    console.error("Failed to send message:", err);
                  }
                }
              }}
              rows={1}
              resize="none"
              fontSize="md"
              bg="gray.50"
              border="2px solid"
              borderColor="gray.200"
              borderRadius="xl"
              _hover={{ borderColor: 'gray.300' }}
              _focus={{ borderColor: 'blue.400', boxShadow: '0 0 0 1px var(--chakra-colors-blue-400)', bg: 'white' }}
              minH="44px"
              maxH="80px"
              p={2}
              flex={1}
            />
            <IconButton
              aria-label="Send message"
              icon={<Send size={18} />}
              colorScheme="blue"
              isDisabled={!userMessage.trim()}
              onClick={async () => {
                if (!userMessage.trim()) return;
                try {
                  const { invoke } = await import("@tauri-apps/api/core");
                  await invoke("send_user_message", { message: userMessage.trim() });
                  toast({ title: "Message sent", status: "success", duration: 2000 });
                  setUserMessage("");
                } catch (err) {
                  console.error("Failed to send message:", err);
                }
              }}
              size="md"
              borderRadius="xl"
              h="44px"
              w="44px"
            />
            <IconButton
              aria-label="Stop execution"
              icon={<Square size={18} />}
              colorScheme="red"
              variant="outline"
              onClick={async () => {
                await emit("stop_automation_request", { automationId });
                onStop();
              }}
              size="md"
              borderRadius="xl"
              h="44px"
              w="44px"
            />
          </HStack>
        ) : onContinue && currentRunId ? (
          <VStack spacing={3} width="100%">
            <HStack spacing={3} width="100%">
              <Textarea
                ref={textareaRef}
                placeholder="Continue this task... (e.g., 'Now also export this to PDF')"
                value={continuationPrompt}
                onChange={(e) => {
                  setContinuationPrompt(e.target.value);
                  e.target.style.height = 'auto';
                  e.target.style.height = `${Math.min(e.target.scrollHeight, 120)}px`;
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
                minH="60px"
                maxH="120px"
                p={3}
              />
              <IconButton
                aria-label="Continue task"
                icon={isContinuing ? <Spinner size="sm" /> : <Send size={20} />}
                colorScheme="blue"
                onClick={handleContinue}
                isDisabled={!continuationPrompt.trim() || isContinuing}
                size="lg"
                borderRadius="xl"
                h="60px"
                w="60px"
              />
            </HStack>
            <Button
              variant="ghost"
              size="sm"
              color="gray.500"
              onClick={() => onClose?.()}
            >
              Close without continuing
            </Button>
          </VStack>
        ) : (
          <Button
            colorScheme="blue"
            size="lg"
            width="100%"
            onClick={() => onClose?.()}
          >
            Close
          </Button>
        )}
      </Box>
    </VStack>
  );
};
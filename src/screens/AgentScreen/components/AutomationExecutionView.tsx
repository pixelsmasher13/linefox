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
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";
import { Square, CheckCircle, AlertCircle, Clock, Send } from "lucide-react";
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

      setCurrentPhase({
        phaseName,
        phaseNumber,
        goal,
        steps: steps || [],
        nextPhaseHint,
      });
    });

    return () => {
      unsubscribe.then((fn) => fn());
    };
  }, [isPlaying]);

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
              {steps.length} {steps.length === 1 ? "Step" : "Steps"} Executed
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
          {/* Chat-style conversation */}
          {completionMessage && objective && (
            <VStack spacing={3} align="stretch" mb={4}>
              {/* User message (objective) */}
              <Flex justify="flex-end">
                <Box
                  bg="white"
                  padding={3}
                  borderRadius="2xl"
                  maxW="80%"
                  boxShadow="sm"
                  border="1px solid"
                  borderColor="gray.200"
                >
                  <ChakraText
                    fontSize="sm"
                    color="gray.800"
                    whiteSpace="pre-wrap"
                    lineHeight="1.5"
                  >
                    {objective}
                  </ChakraText>
                </Box>
              </Flex>

              {/* AI response (completion message) */}
              <Flex justify="flex-start">
                <Box
                  bg="white"
                  padding={3}
                  borderRadius="2xl"
                  maxW="80%"
                  boxShadow="sm"
                  border="1px solid"
                  borderColor="gray.200"
                >
                  <MarkdownContent content={completionMessage!} />
                </Box>
              </Flex>
            </VStack>
          )}

          {/* Current Phase Plan (Agent Mode) */}
          {currentPhase && (
            <Box
              bg="purple.50"
              padding={4}
              borderRadius="md"
              border="1px solid"
              borderColor="purple.200"
              mb={4}
            >
              <VStack align="start" spacing={3}>
                <HStack>
                  <Badge colorScheme="purple" fontSize="sm">
                    Phase {currentPhase.phaseNumber}
                  </Badge>
                  <ChakraText fontWeight="bold" color="purple.800">
                    {currentPhase.phaseName}
                  </ChakraText>
                </HStack>

                <Box>
                  <ChakraText fontSize="xs" fontWeight="bold" color="purple.600" mb={1}>
                    Goal:
                  </ChakraText>
                  <ChakraText fontSize="sm" color="purple.800">
                    {currentPhase.goal}
                  </ChakraText>
                </Box>

                {currentPhase.steps.length > 0 && (
                  <Box width="100%">
                    <ChakraText fontSize="xs" fontWeight="bold" color="purple.600" mb={2}>
                      Plan ({currentPhase.steps.length} steps):
                    </ChakraText>
                    <VStack align="start" spacing={1} pl={2}>
                      {currentPhase.steps.map((step, idx) => (
                        <ChakraText key={idx} fontSize="xs" color="purple.700">
                          {idx + 1}. {step}
                        </ChakraText>
                      ))}
                    </VStack>
                  </Box>
                )}

                {currentPhase.nextPhaseHint && (
                  <ChakraText fontSize="xs" color="purple.500" fontStyle="italic">
                    Next: {currentPhase.nextPhaseHint}
                  </ChakraText>
                )}
              </VStack>
            </Box>
          )}

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
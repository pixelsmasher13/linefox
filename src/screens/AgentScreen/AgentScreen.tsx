import React, { type FC, useState, useEffect } from "react";
import { Flex, Box, IconButton, useToast, Button, Progress, Tooltip, Input, Menu, MenuButton, MenuList, MenuItem, MenuDivider, Avatar, useDisclosure, Modal, ModalOverlay, ModalContent, ModalHeader, ModalBody, ModalFooter, Alert, AlertIcon, VStack, HStack, Spinner, Textarea, Center, Image, Text as ChakraText } from "@chakra-ui/react";
import { Text, NavButton } from "@heelix-app/design";
import styled from "styled-components";
import { keyframes } from '@emotion/react';
import logoBlack from '@heelix-app/design/logo/logo-black.png';
import { ScreenContainer } from "@/components/layout";
import { Play, Settings, Edit, Square, ArrowRight, Send, Home, Plus, Zap, History, PanelLeftClose, PanelLeft, Search, X, CircleDot, FileText, Copy, Check, Bot, Calendar } from "lucide-react";
import { AutomationScript } from "./types";
import { AutomationScriptEditor } from "./components/AutomationScriptEditor";
import { useGlobalSettings } from "../../Providers/SettingsProvider";
import { useRecordingState } from "../../Providers/RecordingStateProvider";
import { SettingsModal } from "./components/SettingsModal";
import { ObjectiveModal } from "./components/ObjectiveModal";
import { useAutomation } from "../../state/automationState";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { UserTakeoverModal } from "./components/UserTakeoverModal";
import { ClarificationModal } from "./components/ClarificationModal";
import { TerminalApprovalDialog } from "../../components/TerminalApprovalDialog";
import { IntakeWizardModal } from "./components/IntakeWizardModal";
import { RecordingProcessingModal } from "./components/RecordingProcessingModal";
import { AutomationExecutionView } from "./components/AutomationExecutionView";
import { ExecutionHistoryView, ExecutionRunWithSteps } from "./components/ExecutionHistoryView";
import { ClipboardModal } from "./components/ClipboardModal";
import { ExecutionDetailsView, ContinuationContext } from "./components/ExecutionDetailsView";
import { SkillsView, Skill } from "./components/SkillsView";
import { SkillEditor } from "./components/SkillEditor";
import { AutomationsList } from "./components/AutomationsList";
import { ScheduleTaskModal } from "./components/ScheduleTaskModal";
import { WeeklyCalendarView } from "./components/WeeklyCalendarView";
import { ChatView } from "./components/ChatView";
import { useChatMessages } from "./hooks/useChatMessages";
import { createMessage } from "./chatTypes";
import { executionToMessages } from "./hooks/executionToMessages";
import { AccessibilityPermissionPrompt } from "../../components/AccessibilityPermissionPrompt";
import { UpdateModal } from "../../components/UpdateModal";

// Create a pulsing animation for the recording indicator
const pulse = keyframes`
  0% { opacity: 0.7; transform: scale(0.95); }
  50% { opacity: 1; transform: scale(1.05); }
  100% { opacity: 0.7; transform: scale(0.95); }
`;

const Sidebar = styled.div<{ $collapsed?: boolean }>`
  grid-area: sidebar;
  height: 100%;
  border-right: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  position: relative;
  width: ${props => props.$collapsed ? '60px' : '300px'};
  transition: width 0.2s ease;
`;

const SidebarContent = styled.div`
  padding: var(--space-m);
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  
  &::-webkit-scrollbar {
    width: 6px;
  }
  
  &::-webkit-scrollbar-track {
    background: transparent;
  }
  
  &::-webkit-scrollbar-thumb {
    background: rgba(0, 0, 0, 0.2);
    border-radius: 3px;
  }
  
  &::-webkit-scrollbar-thumb:hover {
    background: rgba(0, 0, 0, 0.3);
  }
`;

const ContentContainer = styled.div`
  display: flex;
  grid-area: content;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: linear-gradient(180deg, #ffffff 0%, #fafbfc 100%);
`;

const AutomationContainer = styled.div`
  display: flex;
  flex: 1;
  width: 100%;
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  &::-webkit-scrollbar {
    width: 8px;
  }
  &::-webkit-scrollbar-track {
    background: rgba(0, 0, 0, 0.05);
  }
  &::-webkit-scrollbar-thumb {
    background: rgba(0, 0, 0, 0.2);
    border-radius: 4px;
  }
  &::-webkit-scrollbar-thumb:hover {
    background: rgba(0, 0, 0, 0.3);
  }
`;

const MainContent = styled.div`
  flex: 1;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
`;

const AutomationHeader = styled.div`
  width: 100%;
  padding: var(--space-l) var(--space-xl);
  border-bottom: 1px solid var(--color-border);
  display: flex;
  justify-content: space-between;
  align-items: center;
`;

const AutomationContent = styled.div`
  flex: 1;
  padding: var(--space-xl);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
`;

const RunButtonContainer = styled.div`
  margin-top: auto;
  padding-top: var(--space-xl);
  display: flex;
  justify-content: center;
`;


const SidebarActions = styled.div`
  padding: var(--space-m);
  margin-top: auto;
  border-top: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  gap: var(--space-s);
  flex-shrink: 0;
  background-color: white;
`;

const EmptyStateContainer = styled(Flex)`
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  padding: var(--space-xl);
  position: relative;
`;

const LogoContainer = styled.div`
  margin-bottom: var(--space-m);
`;

const HeelixMessage = styled.div`
  text-align: center;
  max-width: 400px;
`;

const Header = styled.div`
  grid-area: header;
  height: 56px;
  background-color: white;
  border-bottom: 1px solid var(--color-border);
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 0 var(--space-m);
  z-index: 10;
`;

const NewAutomationMessage: FC<{
  onRecordClick: () => void;
  onAIGenerate: (prompt: string, agentMode: boolean) => Promise<void>;
  isRecording: boolean;
  isGeneratingAI: boolean;
}> = ({ onRecordClick, onAIGenerate, isRecording, isGeneratingAI }) => {
  const [aiPrompt, setAiPrompt] = useState("");
  const [agentMode, setAgentMode] = useState(false);

  return (
    <EmptyStateContainer>
      <VStack spacing={4}>
        <LogoContainer>
          <Image
            width="48px"
            height="48px"
            src={logoBlack}
            style={{
              opacity: 0.8,
              animation: `${pulse} 3s infinite ease-in-out`
            }}
          />
        </LogoContainer>

        <HeelixMessage>
          <Box mb={2}>
            <Text type="l" bold>
              Welcome to Linefox
            </Text>
          </Box>
          <Box mt={3}>
            <Text type="s" secondary>
              Select a task from the sidebar, or just tell me what to do below
            </Text>
          </Box>
        </HeelixMessage>
      </VStack>

      {/* Input area at bottom */}
      <VStack width="100%" position="absolute" bottom="20px" left="0" right="0" spacing={4} px={8}>
        <VStack width="100%" maxW="600px" mx="auto" spacing={2}>
          <Flex width="100%" gap={2} align="flex-end">
            <Textarea
              placeholder="Tell me what you want done and I'll figure out the rest..."
              value={aiPrompt}
              onChange={(e) => {
                setAiPrompt(e.target.value);
                e.target.style.height = 'auto';
                e.target.style.height = `${e.target.scrollHeight}px`;
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                  e.preventDefault();
                  if (aiPrompt.trim()) {
                    onAIGenerate(aiPrompt, agentMode).then(() => {
                      setAiPrompt("");
                      const textarea = e.target as HTMLTextAreaElement;
                      textarea.style.height = 'auto';
                    });
                  }
                }
              }}
              size="lg"
              rows={1}
              resize="none"
              isDisabled={isGeneratingAI}
              _placeholder={{
                color: 'gray.400',
              }}
              sx={{
                fontSize: '16px',
                borderRadius: '12px',
                border: '1px solid',
                borderColor: 'gray.200',
                backgroundColor: 'white',
                minHeight: '52px',
                maxHeight: '140px',
                overflowY: 'auto',
                flex: 1,
                '&:focus': {
                  borderColor: 'blue.300',
                  boxShadow: '0 0 0 1px var(--chakra-colors-blue-300)',
                },
              }}
            />
            <IconButton
              aria-label="Send"
              icon={<Send size={20} />}
              size="lg"
              isDisabled={!aiPrompt.trim() || isGeneratingAI}
              onClick={() => {
                if (aiPrompt.trim()) {
                  onAIGenerate(aiPrompt, agentMode).then(() => {
                    setAiPrompt("");
                  });
                }
              }}
              sx={{
                backgroundColor: aiPrompt.trim() ? 'blue.500' : 'gray.100',
                color: aiPrompt.trim() ? 'white' : 'gray.400',
                borderRadius: '12px',
                minW: '52px',
                h: '52px',
                '&:hover:not(:disabled)': {
                  backgroundColor: aiPrompt.trim() ? 'blue.600' : 'gray.200',
                },
                '&:disabled': {
                  opacity: 0.6,
                  cursor: 'not-allowed',
                },
              }}
            />
          </Flex>

          {/* Agent Mode Toggle */}
          <Flex width="100%" justify="flex-start" align="center" gap={2} px={1}>
            <Tooltip
              label={agentMode
                ? "Agent Mode: AI will break complex tasks into phases and adapt as it goes. Best for research, multi-step tasks, and ambiguous objectives."
                : "Task Mode: AI executes a predefined script. Best for simple, repeatable tasks."
              }
              placement="top"
              hasArrow
            >
              <Flex
                align="center"
                gap={2}
                cursor="pointer"
                onClick={() => setAgentMode(!agentMode)}
                opacity={isGeneratingAI ? 0.5 : 1}
                pointerEvents={isGeneratingAI ? 'none' : 'auto'}
              >
                <Box
                  width="36px"
                  height="20px"
                  borderRadius="full"
                  bg={agentMode ? "blue.500" : "gray.200"}
                  position="relative"
                  transition="all 0.2s"
                >
                  <Box
                    position="absolute"
                    top="2px"
                    left={agentMode ? "18px" : "2px"}
                    width="16px"
                    height="16px"
                    borderRadius="full"
                    bg="white"
                    transition="all 0.2s"
                    boxShadow="0 1px 2px rgba(0,0,0,0.2)"
                  />
                </Box>
                <Text type="xs" secondary>
                  {agentMode ? "Agent Mode" : "Task Mode"}
                </Text>
              </Flex>
            </Tooltip>
          </Flex>
        </VStack>
      </VStack>
    </EmptyStateContainer>
  );
};

/** Extracts a short tagline from a role's content prompt.
 * Looks for the "— tagline." pattern on the first line; falls back to a 60-char truncation. */
const getRoleTagline = (content: string): string => {
  const firstLine = content.split('\n')[0];
  const match = firstLine.match(/\u2014\s*([^.]+)/);
  if (match) return match[1].trim().replace(/^(.)/, c => c.toUpperCase());
  return firstLine.length > 60 ? `${firstLine.slice(0, 57)}...` : firstLine;
};

export const AutomationScreen: FC = () => {
  const { 
    state: {
      automations,
      selectedAutomationId,
      automationScript,
      isLoadingScript,
      isPlaying,
      isEditing,
      isRecording,
      isProcessingRecording
    },
    selectAutomation,
    playAutomation,
    stopPlayback,
    saveAutomationScript,
    deleteAutomation,
    setIsEditing,
    startRecording,
    stopRecording,
    fetchAutomations,
    fetchAutomationHistory
  } = useAutomation();
  
  const [currentRecordingName, setCurrentRecordingName] = useState<string>("");
  const [additionalInstructions, setAdditionalInstructions] = useState<string>("");
  const [agentMode, setAgentMode] = useState<boolean>(false);
  const [selectedExecution, setSelectedExecution] = useState<ExecutionRunWithSteps | null>(null);
  const [showExecutionView, setShowExecutionView] = useState(false);
  const [currentExecutionRunId, setCurrentExecutionRunId] = useState<number | undefined>(undefined);


  const [takeoverRequest, setTakeoverRequest] = useState<{
    isOpen: boolean;
    instructions: string;
    reasoning: string;
    timestamp: string;
  }>({
    isOpen: false,
    instructions: "",
    reasoning: "",
    timestamp: "",
  });
  const [clarificationRequest, setClarificationRequest] = useState<{
    isOpen: boolean;
    question: string;
    reasoning: string;
    timestamp: string;
  }>({
    isOpen: false,
    question: "",
    reasoning: "",
    timestamp: "",
  });
  const [intakeWizard, setIntakeWizard] = useState<{
    isOpen: boolean;
    questions: string[];
    objective: string;
    name: string;
    automationId: number | null;
  }>({
    isOpen: false,
    questions: [],
    objective: "",
    name: "",
    automationId: null,
  });
  const toast = useToast();
  const { settings } = useGlobalSettings();
  const { toggleRecording } = useRecordingState();
  
  const {
    isOpen: isSettingsOpen,
    onOpen: onSettingsOpen,
    onClose: onSettingsClose,
  } = useDisclosure();
  
  const {
    isOpen: isObjectiveModalOpen,
    onOpen: onObjectiveModalOpen,
    onClose: onObjectiveModalClose,
  } = useDisclosure();
  
  const [isGeneratingAI, setIsGeneratingAI] = useState(false);
  const [generationMessage, setGenerationMessage] = useState("");
  const [clipboardContent, setClipboardContent] = useState<string | null>(null);
  const [taskSearchQuery, setTaskSearchQuery] = useState("");
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const [showScheduledOnly, setShowScheduledOnly] = useState(false);
  const [scheduledAutomationIds, setScheduledAutomationIds] = useState<Set<number>>(new Set());
  const [sidebarView, setSidebarView] = useState<'tasks' | 'history' | 'skills' | 'calendar'>('history');
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false);
  const [availableRoles, setAvailableRoles] = useState<Skill[]>([]);
  const [selectedRoleId, setSelectedRoleId] = useState<number | null>(null);
  const [isRoleMenuOpen, setIsRoleMenuOpen] = useState(false);

  // Schedule modal state
  const [scheduleModal, setScheduleModal] = useState<{ isOpen: boolean; automationId: number | null; automationName: string; executionRunId?: number | null }>({
    isOpen: false, automationId: null, automationName: '', executionRunId: null,
  });

  // Chat messages — drives the ChatView. The hook also registers the
  // user_takeover_required / clarification_required Tauri listeners, so the
  // legacy modal-based listeners further down in this file remain as a
  // visual fallback only (they re-emit the same event into modal state).
  const chatMessages = useChatMessages({
    automationId: selectedAutomationId,
    isPlaying,
  });

  // When user selects an execution from history, replay it into the chat
  // and seed the run/automation ids so continuation works from there.
  useEffect(() => {
    if (!selectedExecution) return;
    const fetchAndPopulate = async () => {
      let objective = "";
      try {
        const automation = await invoke<any>("get_automation_by_id", {
          automationId: selectedExecution.run.automation_id,
        });
        if (automation?.objective) objective = automation.objective;
      } catch {
        /* ignore — fallback to empty objective */
      }
      const msgs = executionToMessages(selectedExecution, objective);
      chatMessages.setMessages(msgs);
      chatMessages.setCurrentRunId(selectedExecution.run.id);
      chatMessages.setCurrentAutomationId(selectedExecution.run.automation_id);
    };
    fetchAndPopulate();
  }, [selectedExecution]);


  // Load available roles and saved selection
  useEffect(() => {
    const loadRoles = async () => {
      try {
        const allSkills = await invoke<Skill[]>('get_all_skills');
        setAvailableRoles(allSkills.filter(s => s.skill_type === 'role' && s.is_active));
        // Load saved selection from settings
        try {
          const saved = await invoke<{ setting_value: string }>('get_setting', { key: 'selected_role' });
          const id = parseInt(saved.setting_value);
          if (id > 0) setSelectedRoleId(id);
        } catch { /* no saved role */ }
      } catch (e) {
        console.log('Failed to load roles:', e);
      }
    };
    loadRoles();

    // Refresh when skills change
    const unlisten = listen('skill_changed', () => loadRoles());
    return () => { unlisten.then(fn => fn()); };
  }, []);

  const selectedRole = availableRoles.find(r => r.id === selectedRoleId) || null;

  // Listen for user takeover requests from backend
  useEffect(() => {
    const unlisten = listen("user_takeover_required", (event: any) => {
      console.log("User takeover requested:", event.payload);
      setTakeoverRequest({
        isOpen: true,
        instructions: event.payload.instructions || "Please complete the required action",
        reasoning: event.payload.reasoning || "",
        timestamp: event.payload.timestamp || new Date().toISOString(),
      });
    });
    
    // Cleanup listener on unmount
    return () => {
      unlisten.then((fn: any) => fn());
    };
  }, []);
  
  // Listen for clarification requests from backend
  useEffect(() => {
    const unlisten = listen("clarification_required", (event: any) => {
      console.log("Clarification requested:", event.payload);
      setClarificationRequest({
        isOpen: true,
        question: event.payload.question || "Please provide clarification",
        reasoning: event.payload.reasoning || "",
        timestamp: event.payload.timestamp || new Date().toISOString(),
      });
    });
    
    // Cleanup listener on unmount
    return () => {
      unlisten.then((fn: any) => fn());
    };
  }, []);
  
  
  // Listen for intake wizard requests from backend
  useEffect(() => {
    const unlisten = listen("intake_wizard_open", (event: any) => {
      console.log("Intake wizard requested:", event.payload);
      setIntakeWizard({
        isOpen: true,
        questions: event.payload.questions || [],
        objective: event.payload.objective || "",
        name: event.payload.name || "",
        automationId: event.payload.automationId || null,
      });
    });
    
    // Cleanup listener on unmount
    return () => {
      unlisten.then((fn: any) => fn());
    };
  }, []);

  // Fetch scheduled automation IDs whenever the history tab is active or a schedule changes
  useEffect(() => {
    const fetchScheduledIds = () => {
      invoke<{ automation_id: number }[]>('get_all_schedules')
        .then(schedules => setScheduledAutomationIds(new Set(schedules.map(s => s.automation_id))))
        .catch(() => {});
    };
    fetchScheduledIds();
    const unlisten = listen('schedule_changed', fetchScheduledIds);
    return () => { unlisten.then((fn: any) => fn()); };
  }, []);

  // Track when to show execution view
  useEffect(() => {
    if (isPlaying) {
      setShowExecutionView(true);
    }
  }, [isPlaying]);

  // Capture execution_run_id from automation_started event (before child component mounts)
  useEffect(() => {
    const unlisten = listen("automation_started", (event: any) => {
      if (event.payload && event.payload.execution_run_id) {
        setCurrentExecutionRunId(event.payload.execution_run_id);
      }
    });

    return () => {
      unlisten.then((fn: any) => fn());
    };
  }, []);

  // Auto-close execution view when user selects different automation
  useEffect(() => {
    if (showExecutionView && !isPlaying) {
      setShowExecutionView(false);
    }
  }, [selectedAutomationId]);

  // Listen for automation stopped events with clipboard data
  useEffect(() => {
    const unlisten = listen("automation_stopped", (event: any) => {
      console.log("Automation stopped:", event.payload);
      if (event.payload && event.payload.clipboard) {
        setClipboardContent(event.payload.clipboard);
      }
      // Keep showing execution view after automation stops
      // User can manually close it or switch tabs
    });

    return () => {
      unlisten.then((fn: any) => fn());
    };
  }, []);

  // Handle record button click
  const handleAIGenerate = async (prompt: string, agentModeFromInput: boolean = false) => {
    setIsGeneratingAI(true);

    // Different messages for agent mode vs task mode
    const taskMessages = [
      "Reading your mind...",
      "Mapping out the journey...",
      "Finding the perfect apps...",
      "Teaching the robots...",
      "Adding some magic...",
      "Almost there..."
    ];

    const agentMessages = [
      "Analyzing your objective...",
      "Preparing the agent...",
      "Almost ready..."
    ];

    const messages = agentModeFromInput ? agentMessages : taskMessages;
    setGenerationMessage(messages[0]);

    let messageIndex = 0;
    const messageInterval = setInterval(() => {
      messageIndex = (messageIndex + 1) % messages.length;
      setGenerationMessage(messages[messageIndex]);
    }, agentModeFromInput ? 1000 : 2000);

    // Update the component-level agentMode so the Run button uses it too
    setAgentMode(agentModeFromInput);

    try {
      // Generate a name from the prompt
      const name = prompt.split(' ').slice(0, 5).join(' ');

      // For agent mode: create minimal automation (no script generation)
      // For task mode: generate full synthetic automation with script
      const result = agentModeFromInput
        ? await invoke<{
            success: boolean;
            automation_id?: number;
            error?: string;
            generated_name?: string;
          }>("create_agent_mode_automation", {
            objective: prompt,
            name,
          })
        : await invoke<{
            success: boolean;
            automation_id?: number;
            error?: string;
            generated_name?: string;
          }>("generate_synthetic_automation", {
            prompt,
            name,
            apiKey: null,
            apiChoice: null,
          });

      clearInterval(messageInterval);

      if (result.success && result.automation_id) {
        const finalName = result.generated_name || name;
        toast({
          title: agentModeFromInput ? "Agent task created" : "Task created",
          description: `Starting "${finalName}"...`,
          status: "success",
          duration: 3000,
          isClosable: true,
        });

        // Refresh automations list
        await fetchAutomations();

        // Auto-start the automation after a short delay
        setTimeout(async () => {
          if (result.automation_id) {
            // Select the new automation
            selectAutomation(result.automation_id);

            // Auto-play it
            setTimeout(async () => {
              if (result.automation_id) {
                const rolePrefix = selectedRole ? `[Role: ${selectedRole.name}]\n${selectedRole.content}\n\n` : '';
                const success = await playAutomation(result.automation_id, rolePrefix, agentModeFromInput);
                if (!success) {
                  console.log("Cannot auto-play AI-generated task");
                }
              }
            }, 500);
          }
        }, 500);
      } else {
        throw new Error(result.error || "Failed to generate task");
      }
    } catch (error) {
      console.error("Error generating AI task:", error);
      clearInterval(messageInterval);
      toast({
        title: "Generation failed",
        description: error instanceof Error ? error.message : "Could not create task from your request",
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      clearInterval(messageInterval);
      setIsGeneratingAI(false);
      setGenerationMessage("");
    }
  };

  /**
   * Route a prompt typed in the chat input. Matches heelix's handlePromptSubmit:
   * 1. If there's an active run/task → classify_continuation_prompt
   *    - quick_reply → just add a quick_reply bubble (no orchestrator)
   *    - continuation → invoke continue_automation_task
   * 2. Otherwise → classify_desktop_prompt
   *    - quick_reply → add a quick_reply bubble (no task generated)
   *    - needs_clarification → add a clarification_ask bubble
   *    - new_task / continue_task → handleAIGenerate
   *
   * This is what was missing: previously every prompt went straight to
   * handleAIGenerate, so even "what's up" spun up the orchestrator and the
   * DIRECT_RESPONSE came back as a completion bubble ("1 STEPS EXECUTED").
   */
  const handlePromptSubmit = async (text: string, agentModeFromInput: boolean) => {
    // Show the user message right away so the UI feels responsive.
    chatMessages.addMessage(createMessage("user_prompt", "user", text));

    const runId = chatMessages.currentRunId || currentExecutionRunId;
    const automationId = selectedAutomationId || chatMessages.currentAutomationId;

    // ——— STATE-FIRST: active context → classify continuation vs quick_reply ———
    if (automationId && runId) {
      chatMessages.addMessage(createMessage("planning", "assistant", "Thinking..."));
      setIsGeneratingAI(true);

      const msgs = chatMessages.messages;
      const completionMsg = [...msgs].reverse().find((m) => m.type === "completion");
      const previousCompletionMessage = completionMsg?.content || "";
      const firstUserMsg = msgs.find((m) => m.type === "user_prompt");
      let previousObjective = firstUserMsg?.content || "";
      try {
        const automation = await invoke<any>("get_automation_by_id", { automationId });
        if (automation?.objective) previousObjective = automation.objective;
      } catch (_) { /* fallback */ }
      const stepMsgs = msgs.filter((m) => m.type === "execution_step").slice(-20);
      const recentSteps = stepMsgs.map((m, i) => {
        let desc = `Step ${i + 1}`;
        if (m.content) desc += `: ${m.content}`;
        const meta = m.metadata as any;
        if (meta?.nextStep) desc += ` → ${meta.nextStep}`;
        return desc;
      });
      const recentChat = msgs
        .filter((m) => m.type === "user_prompt" || m.type === "quick_reply")
        .slice(-10)
        .map((m) => `${m.role === "user" ? "User" : "Assistant"}: ${m.content}`);

      let decision: { classification: string; reply: string | null; confidence: number };
      try {
        decision = await invoke("classify_continuation_prompt", {
          prompt: text,
          previousObjective,
          previousCompletionMessage: previousCompletionMessage || null,
          recentSteps: recentSteps.length > 0 ? recentSteps : null,
          chatHistory: recentChat.length > 0 ? recentChat : null,
          isPlaying,
        });
      } catch (error) {
        console.warn("Continuation classifier failed, defaulting to continuation:", error);
        decision = { classification: "continuation", reply: null, confidence: 0.5 };
      }

      if (decision.classification === "quick_reply") {
        chatMessages.removePlanningMessage();
        setIsGeneratingAI(false);
        const reply = decision.reply || "Got it.";
        chatMessages.addMessage(createMessage("quick_reply", "assistant", reply));
        return;
      }

      chatMessages.removePlanningMessage();
      chatMessages.addMessage(createMessage("planning", "assistant", "Planning continuation..."));

      try {
        await invoke("continue_automation_task", {
          automationId,
          executionRunId: runId,
          continuationPrompt: text,
          previousObjective,
          previousCompletionMessage,
          recentSteps,
        });
        setIsGeneratingAI(false);
      } catch (error) {
        console.error("Failed to continue task:", error);
        chatMessages.removePlanningMessage();
        setIsGeneratingAI(false);
        chatMessages.addMessage(
          createMessage("error", "assistant", error instanceof Error ? error.message : "Failed to continue the task.")
        );
      }
      return;
    }

    // ——— NO active context → classify_desktop_prompt ———
    chatMessages.addMessage(createMessage("planning", "assistant", "Thinking..."));
    setIsGeneratingAI(true);

    const msgs = chatMessages.messages;
    const firstUserMsg = msgs.find((m) => m.type === "user_prompt");
    const lastCompletion = [...msgs].reverse().find((m) => m.type === "completion");
    const lastCompletionTimestamp = lastCompletion?.timestamp;
    const timeSinceCompletionMs = lastCompletionTimestamp
      ? Date.now() - new Date(lastCompletionTimestamp).getTime()
      : undefined;
    const recentChat = msgs
      .filter((m) => m.type === "user_prompt" || m.type === "quick_reply")
      .slice(-10)
      .map((m) => `${m.role === "user" ? "User" : "Assistant"}: ${m.content}`);

    let result: {
      classification: string;
      reply: string | null;
      clarification_question: string | null;
      confidence: number;
    };
    try {
      result = await invoke("classify_desktop_prompt", {
        prompt: text,
        lastObjective: firstUserMsg?.content || null,
        lastCompletionMessage: lastCompletion?.content || null,
        timeSinceCompletionMs: timeSinceCompletionMs || null,
        chatHistory: recentChat.length > 0 ? recentChat : null,
      });
    } catch (error) {
      console.error("Classification failed, falling back to direct generation:", error);
      chatMessages.removePlanningMessage();
      setIsGeneratingAI(false);
      await handleAIGenerate(text, agentModeFromInput);
      return;
    }

    chatMessages.removePlanningMessage();
    setIsGeneratingAI(false);

    try {
      switch (result.classification) {
        case "quick_reply": {
          const reply = result.reply || "I can help with that! What desktop task would you like me to perform?";
          chatMessages.addMessage(createMessage("quick_reply", "assistant", reply));
          break;
        }
        case "needs_clarification": {
          const question = result.clarification_question || "Could you provide more details?";
          chatMessages.addMessage(
            createMessage("clarification_ask", "assistant", question, {
              question,
              reasoning: "Pre-task clarification: the request is ambiguous",
              responded: false,
              preTaskClarification: true,
              originalPrompt: text,
              agentMode: agentModeFromInput,
            } as any)
          );
          break;
        }
        case "continue_task":
        case "new_task":
        default: {
          await handleAIGenerate(text, agentModeFromInput);
          break;
        }
      }
    } catch (error) {
      console.error(`Task dispatch failed (${result.classification}):`, error);
      chatMessages.removePlanningMessage();
      setIsGeneratingAI(false);
      chatMessages.addMessage(
        createMessage("error", "assistant", error instanceof Error ? error.message : "Failed to handle request")
      );
    }
  };

  const handleRecordClick = () => {
    console.log("🔴 Record button clicked. Current state:", { isRecording, isProcessingRecording });
    if (isRecording) {
      console.log("🛑 Stopping recording...");
      handleStopRecording();
    } else {
      console.log("🟢 Opening objective modal to start recording...");
      onObjectiveModalOpen();
    }
  };
  
  // Start recording a new automation with the provided objective
  const handleStartRecording = async (objective: string, name: string) => {
    console.log("🟢 handleStartRecording called with:", { objective, name });
    console.log("🟢 Current recording state before start:", { isRecording, isProcessingRecording });
    
    if (settings.api_choice === "openai" && !settings.api_key_open_ai) {
      toast({
        title: "API key not provided",
        description: "Provide the necessary keys in Settings to continue",
        status: "error",
        duration: 5000,
        isClosable: true,
      });
      onSettingsOpen();
      return;
    }
    
    const success = await startRecording(objective, name);
    console.log("🟢 startRecording returned:", success);
    
    if (success) {
      setCurrentRecordingName(name);
      toggleRecording(true); // Update recording state in RecordingStateProvider
      console.log("🟢 Recording started successfully, state should be updated");
      toast({
        title: "Recording started",
        description: `Recording "${name}" - perform the actions for this task`,
        status: "info",
        duration: 5000,
        isClosable: true,
      });
    } else {
      console.error("❌ Failed to start recording");
      toast({
        title: "Error",
        description: "Failed to start recording",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };
  
  // Stop recording
  const handleStopRecording = async () => {
    console.log("⏹️ handleStopRecording called");
    const success = await stopRecording();
    console.log("⏹️ stopRecording result:", success);
    
    if (success) {
      toggleRecording(false);
      setCurrentRecordingName("");
      console.log("✅ Recording stopped successfully, state reset");
      toast({
        title: "Recording completed",
        description: "Your task has been saved",
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } else {
      console.error("❌ Failed to stop recording");
      toast({
        title: "Error",
        description: "Failed to stop recording",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };
  
  // Stop playback
  const handleStopPlayback = async () => {
    const success = await stopPlayback();
    
    if (!success) {
      // Don't show error toast - the backend might still be processing
      console.log("Stop playback request sent, waiting for backend to respond");
    } else {
      toast({
        title: "Agent Stopped",
        description: "The agent has been stopped",
        status: "info",
        duration: 3000,
        isClosable: true,
      });
    }
  };
  
  // Save edited automation script
  const handleSaveAutomationScript = async (script: AutomationScript) => {
    const success = await saveAutomationScript(script);
    
    if (success) {
      toast({
        title: "Script saved",
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } else {
      toast({
        title: "Error",
        description: "Failed to save script",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };
  
  // Delete automation
  const handleDeleteAutomation = async (automationId: number) => {
    const success = await deleteAutomation(automationId);
    
    if (success) {
      toast({
        title: "Task deleted",
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } else {
      toast({
        title: "Error",
        description: "Failed to delete task",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };
  
  // Handle instructions change
  const handleInstructionsChange = (instructions: string) => {
    setAdditionalInstructions(instructions);
  };

  // Handle share automation
  const handleShareAutomation = (id: number) => {
    toast({
      title: "Feature removed",
      description: "Sharing is not available in the open source version",
      status: "info",
      duration: 3000,
      isClosable: true,
    });
  };


  return (
    <ScreenContainer>
      <Header>
        <Box /> {/* Empty left side */}
        <Flex gap={2} align="center">
          {/* Role selector - next to Home */}
          {availableRoles.length > 0 && (
            <Box position="relative">
              <Tooltip label={selectedRole ? `Role: ${selectedRole.name}` : 'Select role'} placement="bottom">
                <IconButton
                  aria-label="Select role"
                  icon={<Bot size={20} />}
                  variant="ghost"
                  color={selectedRole ? 'purple.500' : undefined}
                  onClick={() => setIsRoleMenuOpen(!isRoleMenuOpen)}
                />
              </Tooltip>
              {selectedRole && (
                <Box position="absolute" top="6px" right="6px" w="8px" h="8px" borderRadius="full" bg="purple.500" border="2px solid white" />
              )}
              {isRoleMenuOpen && (
                <Box
                  position="absolute"
                  top="100%"
                  right="0"
                  mt={1}
                  bg="white"
                  border="1px solid"
                  borderColor="gray.200"
                  borderRadius="md"
                  boxShadow="lg"
                  zIndex={20}
                  minW="180px"
                  py={1}
                >
                  <Box as="button" display="block" width="100%" textAlign="left" px={3} py={2} fontSize="sm" color="gray.500" bg={!selectedRoleId ? 'purple.50' : 'transparent'} _hover={{ bg: 'gray.50' }} onClick={async () => { setSelectedRoleId(null); setIsRoleMenuOpen(false); try { await invoke('set_setting', { key: 'selected_role', value: '0' }); } catch {} }}>
                    No role
                  </Box>
                  {availableRoles.map(role => (
                    <Tooltip key={role.id} label={role.description || getRoleTagline(role.content)} placement="left" openDelay={500}>
                      <Box as="button" display="block" width="100%" textAlign="left" px={3} py={2} fontSize="sm" bg={selectedRoleId === role.id ? 'purple.50' : 'transparent'} color={selectedRoleId === role.id ? 'purple.600' : 'gray.700'} _hover={{ bg: 'gray.50' }} onClick={async () => { setSelectedRoleId(role.id); setIsRoleMenuOpen(false); try { await invoke('set_setting', { key: 'selected_role', value: String(role.id) }); } catch {} }}>
                        {role.name}
                      </Box>
                    </Tooltip>
                  ))}
                </Box>
              )}
            </Box>
          )}
          <Tooltip label="Home">
            <IconButton
              aria-label="Home"
              icon={<Home size={20} />}
              variant="ghost"
              onClick={() => {
                selectAutomation(null);
                setSelectedExecution(null);
                setShowExecutionView(false);
                setAdditionalInstructions("");
              }}
            />
          </Tooltip>
          <Tooltip label="Settings">
            <IconButton
              aria-label="Settings"
              icon={<Settings size={20} />}
              variant="ghost"
              onClick={onSettingsOpen}
            />
          </Tooltip>
        </Flex>
      </Header>
      
      <Sidebar $collapsed={isSidebarCollapsed}>
        {/* Sidebar Navigation */}
        <VStack spacing={1} p={3} align="stretch" borderBottom="1px solid" borderColor="gray.100">
          {/* Top row: Collapse and Home */}
          <HStack spacing={1}>
            <Tooltip label={isSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"} placement="right">
              <IconButton
                aria-label={isSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
                icon={isSidebarCollapsed ? <PanelLeft size={14} /> : <PanelLeftClose size={14} />}
                size="sm"
                variant="ghost"
                color="gray.600"
                _hover={{ bg: 'gray.100' }}
                onClick={() => setIsSidebarCollapsed(!isSidebarCollapsed)}
              />
            </Tooltip>
            {!isSidebarCollapsed && (
              <Button
                leftIcon={<Home size={14} />}
                variant="ghost"
                size="sm"
                color="gray.600"
                _hover={{ bg: 'gray.100' }}
                justifyContent="flex-start"
                flex={1}
                onClick={() => {
                  setSelectedExecution(null);
                  selectAutomation(null);
                  setSelectedSkill(null);
                  setIsEditing(false);
                  setShowExecutionView(false);
                  chatMessages.clearMessages();
                  setSidebarView('tasks');
                }}
              >
                Home
              </Button>
            )}
          </HStack>
          
          {/* Navigation buttons */}
          {!isSidebarCollapsed && (
            <>
              {/* + New Task - above Tasks */}
              <Button
                leftIcon={<Plus size={14} />}
                variant="ghost"
                size="sm"
                justifyContent="flex-start"
                color="gray.600"
                fontWeight="medium"
                _hover={{ bg: 'gray.100' }}
                onClick={() => {
                  setSelectedExecution(null);
                  selectAutomation(null);
                  setSelectedSkill(null);
                  setIsEditing(false);
                  setShowExecutionView(false);
                  chatMessages.clearMessages();
                  setSidebarView('history');
                }}
              >
                New Task
              </Button>
              <Button
                leftIcon={<History size={14} />}
                variant="ghost"
                bg={sidebarView === 'history' ? 'blue.50' : 'transparent'}
                color={sidebarView === 'history' ? 'blue.600' : 'gray.600'}
                _hover={{ bg: sidebarView === 'history' ? 'blue.100' : 'gray.100' }}
                size="sm"
                justifyContent="flex-start"
                onClick={() => setSidebarView('history')}
              >
                Tasks
              </Button>
              <Button
                leftIcon={<FileText size={14} />}
                variant="ghost"
                bg={sidebarView === 'tasks' ? 'blue.50' : 'transparent'}
                color={sidebarView === 'tasks' ? 'blue.600' : 'gray.600'}
                _hover={{ bg: sidebarView === 'tasks' ? 'blue.100' : 'gray.100' }}
                size="sm"
                justifyContent="flex-start"
                onClick={() => setSidebarView('tasks')}
              >
                Task Plans
              </Button>
              <Button
                leftIcon={<Zap size={14} />}
                variant="ghost"
                bg={sidebarView === 'skills' ? 'blue.50' : 'transparent'}
                color={sidebarView === 'skills' ? 'blue.600' : 'gray.600'}
                _hover={{ bg: sidebarView === 'skills' ? 'blue.100' : 'gray.100' }}
                size="sm"
                justifyContent="flex-start"
                onClick={() => setSidebarView('skills')}
              >
                Skills
              </Button>
              <Button
                leftIcon={<Calendar size={14} />}
                variant="ghost"
                bg={sidebarView === 'calendar' ? 'blue.50' : 'transparent'}
                color={sidebarView === 'calendar' ? 'blue.600' : 'gray.600'}
                _hover={{ bg: sidebarView === 'calendar' ? 'blue.100' : 'gray.100' }}
                size="sm"
                justifyContent="flex-start"
                onClick={() => {
                  setSidebarView('calendar');
                  setSelectedSkill(null);
                  setSelectedExecution(null);
                  selectAutomation(null);
                }}
              >
                Calendar
              </Button>
              {/* Search - only show when on history view */}
              {sidebarView === 'history' && (
                isSearchOpen ? (
                  <HStack spacing={1} px={2}>
                    <Search size={14} color="#9ca3af" style={{ flexShrink: 0 }} />
                    <Input
                      size="sm"
                      placeholder="Search tasks..."
                      value={taskSearchQuery}
                      onChange={(e) => setTaskSearchQuery(e.target.value)}
                      autoFocus
                      autoComplete="off"
                      variant="unstyled"
                      sx={{
                        fontSize: '14px',
                        height: '32px',
                        _placeholder: { color: 'gray.400' }
                      }}
                    />
                    <IconButton
                      aria-label="Close search"
                      icon={<X size={14} />}
                      size="xs"
                      variant="ghost"
                      color="gray.400"
                      onClick={() => {
                        setIsSearchOpen(false);
                        setTaskSearchQuery("");
                      }}
                      _hover={{ color: 'gray.600' }}
                    />
                  </HStack>
                ) : (
                  <Button
                    leftIcon={<Search size={14} />}
                    variant="ghost"
                    size="sm"
                    justifyContent="flex-start"
                    color="gray.600"
                    _hover={{ bg: 'gray.100' }}
                    onClick={() => setIsSearchOpen(true)}
                  >
                    Search
                  </Button>
                )
              )}
            </>
          )}
        </VStack>
        
        <SidebarContent>
          {/* Content - hidden when collapsed */}
          {!isSidebarCollapsed && (
            <>
              {sidebarView === 'tasks' && (
                <>
                  {/* Task Plans header with record button */}
                  <HStack justify="space-between" align="center" mb={2} px={1}>
                    <ChakraText fontSize="xs" color="gray.400" fontWeight="medium" textTransform="uppercase" letterSpacing="wider">
                      My Task Plans
                    </ChakraText>
                    <Tooltip label="Record new task" placement="top">
                      <IconButton
                        aria-label="Record new task"
                        icon={<CircleDot size={12} />}
                        size="xs"
                        variant="ghost"
                        color="gray.400"
                        _hover={{ color: 'red.500', bg: 'red.50' }}
                        onClick={handleRecordClick}
                      />
                    </Tooltip>
                  </HStack>
                  
                  {/* Automations List (task templates) */}
                  <Box flex={1} overflow="auto">
                    <AutomationsList
                      automations={automations}
                      selectedAutomationId={selectedAutomationId}
                      onSelectAutomation={(id) => {
                        selectAutomation(id);
                        setSelectedExecution(null);
                        setSelectedSkill(null);
                      }}
                      onDeleteAutomation={deleteAutomation}
                      onScheduleAutomation={(id, name) => setScheduleModal({ isOpen: true, automationId: id, automationName: name })}
                      isActive={sidebarView === 'tasks'}
                    />
                  </Box>
                </>
              )}

              {sidebarView === 'history' && (
                <>
                  {/* My Tasks header */}
                  <HStack justify="space-between" align="center" mb={2} px={1}>
                    <ChakraText fontSize="xs" color="gray.400" fontWeight="medium" textTransform="uppercase" letterSpacing="wider">
                      My Tasks
                    </ChakraText>
                    <Tooltip label={showScheduledOnly ? "Show all tasks" : "Show scheduled only"} placement="top">
                      <IconButton
                        aria-label="Filter scheduled tasks"
                        icon={<Calendar size={12} />}
                        size="xs"
                        variant="ghost"
                        color={showScheduledOnly ? "blue.500" : "gray.400"}
                        bg={showScheduledOnly ? "blue.50" : "transparent"}
                        _hover={{ color: "blue.500", bg: "blue.50" }}
                        onClick={() => setShowScheduledOnly(prev => !prev)}
                      />
                    </Tooltip>
                  </HStack>
                  
                  {/* Execution History */}
                  <Box flex={1} overflow="auto">
                    <ExecutionHistoryView
                      automations={automations}
                      limit={50}
                      selectedExecutionId={selectedExecution?.run.id}
                      isVisible={true}
                      searchQuery={taskSearchQuery}
                      scheduledAutomationIds={scheduledAutomationIds}
                      showScheduledOnly={showScheduledOnly}
                      onSelectExecution={(execution) => {
                        setSelectedExecution(execution);
                        setSelectedSkill(null);
                        selectAutomation(null);
                      }}
                    />
                  </Box>
                </>
              )}

              {sidebarView === 'skills' && (
                <>
                  {/* My Skills header */}
                  <HStack justify="space-between" align="center" mb={2} px={1}>
                    <ChakraText fontSize="xs" color="gray.400" fontWeight="medium" textTransform="uppercase" letterSpacing="wider">
                      My Skills
                    </ChakraText>
                  </HStack>
                  
                  {/* Skills View */}
                  <SkillsView 
                    onSelectSkill={(skill) => {
                      setSelectedSkill(skill);
                      setSelectedExecution(null);
                      selectAutomation(null);
                    }}
                  />
                </>
              )}
            </>
          )}
        </SidebarContent>



        {/* Show status indicators at bottom of sidebar */}
        {(isPlaying || isRecording) && (
          <SidebarActions>
            {isPlaying ? (
              <Box
                p={3}
                bg="blue.50"
                borderRadius="md"
                border="1px solid"
                borderColor="blue.200"
              >
                <Flex align="center" justify="space-between">
                  <Flex align="center" gap={2}>
                    <Box
                      as="span"
                      w="8px"
                      h="8px"
                      borderRadius="full"
                      bg="blue.500"
                      animation={`${pulse} 1.5s infinite ease-in-out`}
                    />
                    <Text type="s">
                      Task Running...
                    </Text>
                  </Flex>
                  <IconButton
                    aria-label="Stop task"
                    icon={<Square size={12} fill="currentColor" />}
                    size="xs"
                    variant="ghost"
                    color="gray.500"
                    _hover={{ bg: 'gray.200', color: 'gray.700' }}
                    onClick={handleStopPlayback}
                  />
                </Flex>
              </Box>
            ) : isRecording ? (
              <Box
                p={3}
                bg="red.50"
                borderRadius="md"
                border="1px solid"
                borderColor="red.200"
                cursor="pointer"
                onClick={handleRecordClick}
                _hover={{ bg: 'red.100' }}
              >
                <Flex align="center" gap={2}>
                  <Box
                    as="span"
                    w="8px"
                    h="8px"
                    borderRadius="full"
                    bg="red.500"
                    animation={`${pulse} 1.5s infinite ease-in-out`}
                  />
                  <Text type="s">
                    Recording... Click to stop
                  </Text>
                </Flex>
              </Box>
            ) : null}
          </SidebarActions>
        )}
      </Sidebar>
      
      <ContentContainer>
        {/* Show accessibility permission prompt as a modal when permissions are missing */}
        <AccessibilityPermissionPrompt
          showAsModal={true}
          onPermissionGranted={() => {}}
        />
        
        <AutomationContainer>
          {sidebarView === 'calendar' ? (
            <WeeklyCalendarView
              onEditSchedule={(automationId, automationName) => {
                setScheduleModal({
                  isOpen: true,
                  automationId,
                  automationName,
                  executionRunId: null,
                });
              }}
            />
          ) : selectedSkill ? (
            // Show skill editor when a skill is selected
            <Box width="100%" height="100%" p={8} overflow="auto">
              <Box maxW="800px" mx="auto">
                <SkillEditor
                  skill={selectedSkill}
                  onSave={async (updatedSkill) => {
                    try {
                      if (updatedSkill.id === 0) {
                        // Create new
                        await invoke('create_skill', {
                          name: updatedSkill.name,
                          skillType: updatedSkill.skill_type || 'site',
                          domains: updatedSkill.domains || [],
                          triggers: updatedSkill.triggers || [],
                          description: updatedSkill.description || '',
                          content: updatedSkill.content || '',
                        });
                        toast({
                          title: 'Skill created',
                          status: 'success',
                          duration: 2000,
                        });
                      } else {
                        // Update existing
                        await invoke('update_skill', {
                          skillId: updatedSkill.id,
                          name: updatedSkill.name,
                          skillType: updatedSkill.skill_type || 'site',
                          domains: updatedSkill.domains || [],
                          triggers: updatedSkill.triggers || [],
                          description: updatedSkill.description || '',
                          content: updatedSkill.content || '',
                          isActive: updatedSkill.is_active ?? true,
                        });
                        toast({
                          title: 'Skill updated',
                          status: 'success',
                          duration: 2000,
                        });
                      }
                      setSelectedSkill(null);
                    } catch (error) {
                      console.error('Failed to save skill:', error);
                      toast({
                        title: 'Error saving skill',
                        description: String(error),
                        status: 'error',
                        duration: 3000,
                      });
                    }
                  }}
                  onCancel={() => setSelectedSkill(null)}
                />
              </Box>
            </Box>
          ) : selectedExecution ? (
            // Past execution from history — render it through ChatView so the
            // chat thread, execution header, and continuation flow all match
            // the live-run experience.
            <ChatView
              messages={chatMessages.messages}
              isPlaying={isPlaying}
              isPlanning={isGeneratingAI}
              onSendPrompt={(text, agentModeFromInput) => {
                handlePromptSubmit(text, agentModeFromInput);
              }}
              onSendMessage={async (text) => {
                chatMessages.addMessage(createMessage("user_prompt", "user", text));
                try {
                  await invoke("send_user_message", { message: text.trim() });
                } catch (error) {
                  console.error("Failed to send mid-run message:", error);
                }
              }}
              onStop={handleStopPlayback}
              onContinue={(text) => {
                handlePromptSubmit(text, agentMode);
              }}
              onSubmitClarification={(response) => {
                chatMessages.addMessage(createMessage("clarification_response", "user", response));
                chatMessages.updateLastMessageOfType("clarification_ask", {
                  metadata: { responded: true } as any,
                });
              }}
              onCompleteTakeover={() => {
                chatMessages.addMessage(createMessage("takeover_complete", "system", "Manual action completed — agent resuming..."));
                chatMessages.updateLastMessageOfType("takeover_request", {
                  metadata: { completed: true } as any,
                });
              }}
              executionHeader={{
                taskName: (automations as any[]).find((a: any) => a.id === (selectedExecution as any).run.automation_id)?.name || "Task Details",
                status: (selectedExecution as any).run.status,
                startedAt: (selectedExecution as any).run.started_at,
                completedAt: (selectedExecution as any).run.completed_at ?? undefined,
                automationId: (selectedExecution as any).run.automation_id,
                executionRunId: (selectedExecution as any).run.id,
                additionalInstructions: (selectedExecution as any).run.additional_instructions ?? undefined,
              }}
              onEdit={(automationId) => {
                selectAutomation(automationId);
                setSelectedExecution(null);
                setTimeout(() => setIsEditing(true), 100);
              }}
              onSchedule={(automationId, executionRunId) => {
                const name = (automations as any[]).find((a: any) => a.id === automationId)?.name || "";
                setScheduleModal({ isOpen: true, automationId, automationName: name, executionRunId });
              }}
              onRerun={async (automationId, instructions) => {
                selectAutomation(automationId);
                setAdditionalInstructions(instructions || '');
                setSelectedExecution(null);
                chatMessages.clearMessages();
                setShowExecutionView(true);
                setTimeout(async () => {
                  try {
                    const rolePrefix = selectedRole ? `[Role: ${selectedRole.name}]\n${selectedRole.content}\n\n` : '';
                    await playAutomation(automationId, rolePrefix + (instructions || ''), agentMode);
                  } catch (error) {
                    console.error("Failed to start playback:", error);
                    toast({
                      title: "Error",
                      description: "Failed to start the task. Please try again.",
                      status: "error",
                      duration: 5000,
                      isClosable: true,
                    });
                  }
                }, 100);
              }}
            />
          ) : automationScript && isEditing ? (
            // Script editor — ONLY when explicitly editing. Otherwise the chat
            // view is the default (matches heelix). This is what was hiding the
            // chat: previously this branch fired whenever a script was loaded.
            <MainContent>
              <AutomationHeader>
                <Text type="l" bold>{automationScript.name}</Text>
              </AutomationHeader>
              <AutomationContent>
                <AutomationScriptEditor
                  script={automationScript}
                  isEditing={isEditing}
                  onSave={handleSaveAutomationScript}
                  onCancel={() => setIsEditing(false)}
                  onInstructionsChange={handleInstructionsChange}
                  additionalInstructions={additionalInstructions}
                />
              </AutomationContent>
            </MainContent>
          ) : (
            // Default — ChatView is the home for everything else: empty state,
            // active run, and post-run with executionHeader if a past run is
            // selected from history.
            <ChatView
              messages={chatMessages.messages}
              isPlaying={isPlaying}
              isPlanning={isGeneratingAI}
              onSendPrompt={(text, agentModeFromInput) => {
                handlePromptSubmit(text, agentModeFromInput);
              }}
              onSendMessage={async (text) => {
                chatMessages.addMessage(createMessage("user_prompt", "user", text));
                try {
                  await invoke("send_user_message", { message: text.trim() });
                } catch (error) {
                  console.error("Failed to send mid-run message:", error);
                }
              }}
              onStop={handleStopPlayback}
              onContinue={(text) => {
                handlePromptSubmit(text, agentMode);
              }}
              onSubmitClarification={(response) => {
                chatMessages.addMessage(createMessage("clarification_response", "user", response));
                chatMessages.updateLastMessageOfType("clarification_ask", {
                  metadata: { responded: true } as any,
                });
              }}
              onCompleteTakeover={() => {
                chatMessages.addMessage(createMessage("takeover_complete", "system", "Manual action completed — agent resuming..."));
                chatMessages.updateLastMessageOfType("takeover_request", {
                  metadata: { completed: true } as any,
                });
              }}
              onEdit={(automationId) => {
                selectAutomation(automationId);
                setTimeout(() => setIsEditing(true), 100);
              }}
              onSchedule={(automationId, executionRunId) => {
                const name = (automations as any[]).find((a: any) => a.id === automationId)?.name || "";
                setScheduleModal({ isOpen: true, automationId, automationName: name, executionRunId });
              }}
              onRerun={async (automationId) => {
                selectAutomation(automationId);
                setSelectedExecution(null);
                chatMessages.clearMessages();
                const rolePrefix = selectedRole ? `[Role: ${selectedRole.name}]\n${selectedRole.content}\n\n` : '';
                setTimeout(() => playAutomation(automationId, rolePrefix, false), 300);
              }}
            />
          )}
        </AutomationContainer>
      </ContentContainer>
      
      <ScheduleTaskModal
        isOpen={scheduleModal.isOpen}
        onClose={() => setScheduleModal({ isOpen: false, automationId: null, automationName: '', executionRunId: null })}
        automationId={scheduleModal.automationId}
        automationName={scheduleModal.automationName}
        executionRunId={scheduleModal.executionRunId}
      />

      {isSettingsOpen && (
        <SettingsModal
          isOpen={isSettingsOpen}
          onClose={onSettingsClose}
        />
      )}
      
      <ObjectiveModal 
        isOpen={isObjectiveModalOpen}
        onClose={onObjectiveModalClose}
        onSubmit={(objective, name) => {
          onObjectiveModalClose();
          handleStartRecording(objective, name);
        }}
      />
      
      <UserTakeoverModal
        isOpen={takeoverRequest.isOpen}
        instructions={takeoverRequest.instructions}
        reasoning={takeoverRequest.reasoning}
        timestamp={takeoverRequest.timestamp}
        onComplete={() => {
          setTakeoverRequest({
            isOpen: false,
            instructions: "",
            reasoning: "",
            timestamp: "",
          });
          toast({
            title: "Takeover completed",
            description: "Agent is continuing...",
            status: "success",
            duration: 3000,
            isClosable: true,
          });
        }}
      />
      
      <ClarificationModal
        isOpen={clarificationRequest.isOpen}
        question={clarificationRequest.question}
        reasoning={clarificationRequest.reasoning}
        timestamp={clarificationRequest.timestamp}
        onComplete={() => {
          setClarificationRequest({
            isOpen: false,
            question: "",
            reasoning: "",
            timestamp: "",
          });
          toast({
            title: "Clarification submitted",
            description: "Agent is continuing with your response...",
            status: "success",
            duration: 3000,
            isClosable: true,
          });
        }}
      />
      
      {/* Terminal Approval Dialog - shown when agent wants to run terminal commands */}
      <TerminalApprovalDialog />
      
      <IntakeWizardModal
        isOpen={intakeWizard.isOpen}
        questions={intakeWizard.questions}
        onCancel={() => {
          setIntakeWizard({
            isOpen: false,
            questions: [],
            objective: "",
            name: "",
            automationId: null,
          });
          toast({
            title: "Setup cancelled",
            description: "You can always provide these details later",
            status: "info",
            duration: 3000,
            isClosable: true,
          });
        }}
        onComplete={async (answers) => {
          try {
            // Save the answers to the backend
            if (intakeWizard.automationId) {
              await invoke("save_intake_answers", {
                automationId: intakeWizard.automationId,
                qaPairs: answers.map(a => [a.question, a.answer])
              });
              
              toast({
                title: "Details saved",
                description: "Your clarifications have been saved with the task",
                status: "success",
                duration: 3000,
                isClosable: true,
              });
            }
          } catch (error) {
            console.error("Error saving intake answers:", error);
            toast({
              title: "Error",
              description: "Failed to save your answers",
              status: "error",
              duration: 3000,
              isClosable: true,
            });
          } finally {
            setIntakeWizard({
              isOpen: false,
              questions: [],
              objective: "",
              name: "",
              automationId: null,
            });
          }
        }}
      />
      
      {/* Processing Modal - shown while generating scripts after recording */}
      <RecordingProcessingModal isOpen={isProcessingRecording} />
      

      {/* AI Generation modal removed — ChatView's PlanningIndicator now
          shows the "Thinking..." bubble inline while isGeneratingAI is true. */}

      <UpdateModal />
    </ScreenContainer>
  );
}; 
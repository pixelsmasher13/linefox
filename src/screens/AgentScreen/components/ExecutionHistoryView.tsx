import React, { useEffect, useState, useRef, useMemo, useCallback } from "react";
import {
  VStack,
  HStack,
  Spinner,
  Flex,
  useToast,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  IconButton,
  Text as ChakraText,
} from "@chakra-ui/react";
import { Text } from "@heelix-app/design";
import { CheckCircle, AlertCircle, Clock, Square, MoreHorizontal, Trash2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Automation } from "../types";

export interface ExecutionStep {
  id: number;
  execution_run_id: number;
  step_number: number;
  timestamp: string;
  explanation?: string;
  next_step?: string;
  status: string;
  error_message?: string;
  created_at: string;
  step_type?: string; // 'action' | 'user_prompt' | 'completion'
}

export interface ExecutionRun {
  id: number;
  automation_id: number;
  started_at: string;
  completed_at?: string;
  status: string;
  additional_instructions?: string;
  error_message?: string;
  clipboard?: string;
  completion_message?: string;
  created_at: string;
}

export interface ExecutionRunWithSteps {
  run: ExecutionRun;
  steps: ExecutionStep[];
}

interface ExecutionHistoryViewProps {
  automationId?: number;
  automations?: Automation[];
  limit?: number;
  selectedExecutionId?: number;
  onSelectExecution: (execution: ExecutionRunWithSteps) => void;
  isVisible?: boolean;
  searchQuery?: string;
  scheduledAutomationIds?: Set<number>;
  showScheduledOnly?: boolean;
}

export const ExecutionHistoryView: React.FC<ExecutionHistoryViewProps> = ({
  automationId,
  automations = [],
  limit = 10,
  selectedExecutionId,
  onSelectExecution,
  isVisible = false,
  searchQuery = "",
  scheduledAutomationIds,
  showScheduledOnly = false,
}) => {
  const [executionHistory, setExecutionHistory] = useState<ExecutionRunWithSteps[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const toast = useToast();

  useEffect(() => {
    if (isVisible) {
      fetchExecutionHistory();
    }
  }, [automationId, limit, isVisible]);
  
  // Listen for automation events to auto-refresh the list
  useEffect(() => {
    const unlistenStarted = listen('automation_started', () => {
      fetchExecutionHistory();
    });
    const unlistenCompleted = listen('automation_completed', () => {
      fetchExecutionHistory();
    });
    const unlistenStopped = listen('automation_stopped', () => {
      fetchExecutionHistory();
    });
    
    return () => {
      unlistenStarted.then(fn => fn());
      unlistenCompleted.then(fn => fn());
      unlistenStopped.then(fn => fn());
    };
  }, [automationId, limit]);
  
  // Add keyboard navigation
  useEffect(() => {
    if (!isVisible || !executionHistory.length) return;
    
    const handleKeyDown = (e: KeyboardEvent) => {
      // Only handle if this tab is visible
      if (!isVisible) return;
      
      const currentIndex = selectedExecutionId 
        ? executionHistory.findIndex(ex => ex.run.id === selectedExecutionId)
        : -1;
      let newIndex = currentIndex;
      
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        if (currentIndex === -1) {
          // No selection, select first item
          newIndex = 0;
        } else {
          newIndex = currentIndex < executionHistory.length - 1 ? currentIndex + 1 : 0;
        }
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (currentIndex === -1) {
          // No selection, select last item
          newIndex = executionHistory.length - 1;
        } else {
          newIndex = currentIndex > 0 ? currentIndex - 1 : executionHistory.length - 1;
        }
      } else {
        return;
      }
      
      if (executionHistory[newIndex]) {
        onSelectExecution(executionHistory[newIndex]);
        
        // Scroll the selected item into view
        setTimeout(() => {
          const selectedElement = containerRef.current?.querySelector(`[data-execution-id="${executionHistory[newIndex].run.id}"]`) as HTMLElement;
          selectedElement?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }, 0);
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [executionHistory, selectedExecutionId, onSelectExecution, isVisible]);

  const fetchExecutionHistory = async () => {
    setIsLoading(true);
    setError(null);
    
    try {
      const history = await invoke<ExecutionRunWithSteps[]>("get_automation_execution_history", {
        limit,
      });
      
      // Filter by automation ID if provided
      const filtered = automationId
        ? history.filter(h => h.run.automation_id === automationId)
        : history;
        
      setExecutionHistory(filtered);
    } catch (err) {
      console.error("Error fetching execution history:", err);
      setError("Failed to load execution history");
    } finally {
      setIsLoading(false);
    }
  };

  const handleDeleteExecution = async (e: React.MouseEvent, executionId: number) => {
    e.stopPropagation(); // Prevent selecting the execution
    
    setDeletingId(executionId);
    
    try {
      await invoke("delete_execution_run", { runId: executionId });
      
      // Remove from local state
      setExecutionHistory(prev => 
        prev.filter(execution => execution.run.id !== executionId)
      );
      
      toast({
        title: "Execution deleted",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to delete execution:", error);
      toast({
        title: "Failed to delete execution",
        description: error?.toString() || "Unknown error",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setDeletingId(null);
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case "completed":
        return <CheckCircle size={14} color="#22c55e" />;
      case "running":
      case "executing":
        return <Spinner size="xs" color="blue.400" />;
      case "failed":
      case "error":
        return <AlertCircle size={14} color="#ef4444" />;
      case "stopped":
        return <Square size={14} color="#9ca3af" />;
      case "interrupted":
        return <Clock size={14} color="#9ca3af" />;
      default:
        return <Clock size={14} color="#9ca3af" />;
    }
  };

  const truncateName = (name: string, maxLength: number = 28) => {
    if (name.length <= maxLength) return name;
    return name.substring(0, maxLength) + "...";
  };

  const getAutomationName = (automationId: number) => {
    const automation = automations.find(a => a.id === automationId);
    return automation?.name || `Task #${automationId}`;
  };

  // Filter executions based on search query - MUST be called before any early returns
  const filteredExecutions = useMemo(() => {
    let filtered = executionHistory;

    if (showScheduledOnly) {
      filtered = filtered.filter(e => scheduledAutomationIds?.has(e.run.automation_id) ?? false);
    }

    if (!searchQuery.trim()) return filtered;

    const query = searchQuery.toLowerCase();
    return filtered.filter((execution) => {
      const automationName = getAutomationName(execution.run.automation_id).toLowerCase();
      const instructions = (execution.run.additional_instructions || '').toLowerCase();
      return automationName.includes(query) || instructions.includes(query);
    });
  }, [executionHistory, searchQuery, automations, showScheduledOnly, scheduledAutomationIds]);

  if (isLoading) {
    return (
      <Flex justify="center" align="center" height="200px">
        <VStack spacing={4}>
          <Spinner size="lg" color="blue.500" />
          <Text type="m" secondary>
            Loading execution history...
          </Text>
        </VStack>
      </Flex>
    );
  }

  if (error) {
    return (
      <Flex justify="center" align="center" height="200px">
        <VStack spacing={4}>
          <AlertCircle size={40} color="red" />
          <Text type="m" secondary>
            {error}
          </Text>
        </VStack>
      </Flex>
    );
  }

  if (executionHistory.length === 0) {
    return (
      <Flex justify="center" align="center" height="200px">
        <VStack spacing={4}>
          <Clock size={40} color="gray" />
          <Text type="m" secondary>
            No execution history available
          </Text>
        </VStack>
      </Flex>
    );
  }

  if (filteredExecutions.length === 0) {
    return (
      <Flex justify="center" align="center" height="200px">
        <VStack spacing={4}>
          <Clock size={40} color="gray" />
          <Text type="m" secondary>
            No matching history found
          </Text>
        </VStack>
      </Flex>
    );
  }

  return (
    <VStack ref={containerRef} spacing={0} align="stretch" width="100%">
      {filteredExecutions.map((execution) => (
        <HStack
          key={execution.run.id}
          data-execution-id={execution.run.id}
          tabIndex={0}
          px={3}
          py={2}
          spacing={2}
          borderRadius="6px"
          bg={selectedExecutionId === execution.run.id ? "rgba(37, 99, 235, 0.1)" : "transparent"}
          cursor="pointer"
          transition="all 0.1s ease"
          outline="none"
          _hover={{ bg: selectedExecutionId === execution.run.id ? "rgba(37, 99, 235, 0.15)" : "rgba(0, 0, 0, 0.04)" }}
          _focus={{ outline: "none" }}
          onClick={() => onSelectExecution(execution)}
        >
          {getStatusIcon(execution.run.status)}
          <ChakraText fontSize="sm" fontWeight="medium" flex={1} noOfLines={1}>
            {truncateName(getAutomationName(execution.run.automation_id))}
          </ChakraText>
          <Menu>
            <MenuButton
              as={IconButton}
              aria-label="Options"
              icon={<MoreHorizontal size={14} />}
              size="xs"
              variant="ghost"
              opacity={0.5}
              _hover={{ opacity: 1 }}
              onClick={(e: React.MouseEvent) => e.stopPropagation()}
            />
            <MenuList minW="120px" py={1}>
              <MenuItem
                icon={<Trash2 size={14} />}
                onClick={(e: React.MouseEvent) => handleDeleteExecution(e, execution.run.id)}
                isDisabled={deletingId === execution.run.id}
                fontSize="sm"
                color="red.500"
              >
                Delete
              </MenuItem>
            </MenuList>
          </Menu>
        </HStack>
      ))}
    </VStack>
  );
};
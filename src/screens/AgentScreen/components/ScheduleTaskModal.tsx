import React, { useState, useEffect } from 'react';
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  Button,
  FormControl,
  FormLabel,
  RadioGroup,
  Radio,
  Stack,
  Select,
  HStack,
  Box,
  VStack,
  Badge,
  IconButton,
  Switch,
  Textarea,
  useToast,
  Text as ChakraText,
} from '@chakra-ui/react';
import { Text } from '@heelix-app/design';
import { Calendar, Clock, Trash2, GitBranch, RefreshCw } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';

interface ScheduleTaskModalProps {
  isOpen: boolean;
  onClose: () => void;
  automationId: number | null;
  automationName: string;
  executionRunId?: number | null;
  onScheduleChanged?: () => void;
}

interface ExistingSchedule {
  id: number;
  automation_id: number;
  recurrence_type: string;
  recurrence_days: string | null;
  execution_hour: number;
  execution_minute: number;
  timezone: string;
  is_active: boolean;
  next_run_at: string | null;
  last_run_at: string | null;
}

const DAYS_OF_WEEK = [
  { value: 0, label: 'Sun' },
  { value: 1, label: 'Mon' },
  { value: 2, label: 'Tue' },
  { value: 3, label: 'Wed' },
  { value: 4, label: 'Thu' },
  { value: 5, label: 'Fri' },
  { value: 6, label: 'Sat' },
];

const formatHour = (h: number) => {
  if (h === 0) return '12 AM';
  if (h < 12) return `${h} AM`;
  if (h === 12) return '12 PM';
  return `${h - 12} PM`;
};

const HOURS = Array.from({ length: 24 }, (_, i) => ({
  value: i,
  label: `${formatHour(i)} - ${formatHour((i + 1) % 24)}`,
}));

const describeSchedule = (s: ExistingSchedule) => {
  const time = formatHour(s.execution_hour);
  switch (s.recurrence_type) {
    case 'daily':
      return `Daily at ${time}`;
    case 'weekdays':
      return `Weekdays at ${time}`;
    case 'weekly': {
      const days: number[] = s.recurrence_days ? JSON.parse(s.recurrence_days) : [];
      const dayLabels = days.map(d => DAYS_OF_WEEK.find(dw => dw.value === d)?.label || '?').join(', ');
      return `${dayLabels} at ${time}`;
    }
    default:
      return `${s.recurrence_type} at ${time}`;
  }
};

export const ScheduleTaskModal: React.FC<ScheduleTaskModalProps> = ({
  isOpen,
  onClose,
  automationId,
  automationName,
  executionRunId,
  onScheduleChanged,
}) => {
  const [recurrenceType, setRecurrenceType] = useState<'daily' | 'weekdays' | 'weekly'>('daily');
  const [selectedDays, setSelectedDays] = useState<number[]>([1, 3, 5]); // Mon, Wed, Fri
  const [executionHour, setExecutionHour] = useState(9);
  const [useContinuationMode, setUseContinuationMode] = useState(false);
  const [continuationPrompt, setContinuationPrompt] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [existingSchedules, setExistingSchedules] = useState<ExistingSchedule[]>([]);
  const toast = useToast();

  // Default to continuation mode when opened from ExecutionDetailsView
  useEffect(() => {
    if (isOpen && executionRunId) {
      setUseContinuationMode(true);
    }
  }, [isOpen, executionRunId]);

  // Load existing schedules for this automation
  useEffect(() => {
    if (isOpen && automationId) {
      invoke('get_schedules_for_automation', { automationId })
        .then((result: any) => {
          setExistingSchedules(result || []);
        })
        .catch(err => console.error('Failed to load schedules:', err));
    }
  }, [isOpen, automationId]);

  const handleSubmit = async () => {
    if (!automationId) return;

    if (recurrenceType === 'weekly' && selectedDays.length === 0) {
      toast({
        title: 'Select at least one day',
        status: 'warning',
        duration: 3000,
        isClosable: true,
      });
      return;
    }

    setIsSubmitting(true);
    try {
      await invoke('create_schedule', {
        automationId,
        recurrenceType,
        recurrenceDays: recurrenceType === 'weekly' ? selectedDays : null,
        executionHour,
        executionMinute: 0,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        persistentRunId: useContinuationMode && executionRunId ? executionRunId : null,
        continuationPrompt: useContinuationMode && continuationPrompt.trim() ? continuationPrompt.trim() : null,
      });

      toast({
        title: 'Schedule created',
        description: `Task will run ${recurrenceType} at ${formatHour(executionHour)}`,
        status: 'success',
        duration: 3000,
        isClosable: true,
      });

      onScheduleChanged?.();

      // Refresh existing schedules
      const result: any = await invoke('get_schedules_for_automation', { automationId });
      setExistingSchedules(result || []);

      // Reset form
      setRecurrenceType('daily');
      setSelectedDays([1, 3, 5]);
      setExecutionHour(9);
      setContinuationPrompt('');
    } catch (error: any) {
      toast({
        title: 'Failed to create schedule',
        description: error?.toString() || 'Unknown error',
        status: 'error',
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleToggleSchedule = async (scheduleId: number, isActive: boolean) => {
    try {
      await invoke('update_schedule', { scheduleId, isActive });
      setExistingSchedules(prev =>
        prev.map(s => s.id === scheduleId ? { ...s, is_active: isActive } : s)
      );
      onScheduleChanged?.();
    } catch (error) {
      console.error('Failed to toggle schedule:', error);
    }
  };

  const handleDeleteSchedule = async (scheduleId: number) => {
    try {
      await invoke('delete_schedule', { scheduleId });
      setExistingSchedules(prev => prev.filter(s => s.id !== scheduleId));
      onScheduleChanged?.();
      toast({
        title: 'Schedule deleted',
        status: 'info',
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error('Failed to delete schedule:', error);
    }
  };

  const handleDayToggle = (day: number) => {
    setSelectedDays(prev =>
      prev.includes(day)
        ? prev.filter(d => d !== day)
        : [...prev, day].sort((a, b) => a - b)
    );
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} size="md">
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>
          <HStack spacing={2}>
            <Calendar size={20} />
            <Text type="m">Schedule Task</Text>
          </HStack>
        </ModalHeader>
        <ModalCloseButton />

        <ModalBody>
          {automationName && (
            <Box mb={4} p={3} bg="gray.50" borderRadius="md">
              <Text type="s" secondary>Task:</Text>
              <Text type="s">{automationName}</Text>
            </Box>
          )}

          {/* Existing schedules */}
          {existingSchedules.length > 0 && (
            <Box mb={5}>
              <Text type="s" secondary>Active Schedules</Text>
              <VStack spacing={2} mt={2} align="stretch">
                {existingSchedules.map(schedule => (
                  <HStack
                    key={schedule.id}
                    p={2}
                    borderRadius="md"
                    border="1px"
                    borderColor="gray.200"
                    justify="space-between"
                  >
                    <VStack spacing={0} align="start">
                      <Text type="s">{describeSchedule(schedule)}</Text>
                      {schedule.next_run_at && (
                        <Text type="xs" secondary>
                          Next: {new Date(schedule.next_run_at).toLocaleString()}
                        </Text>
                      )}
                    </VStack>
                    <HStack spacing={2}>
                      <Switch
                        size="sm"
                        isChecked={schedule.is_active}
                        onChange={(e) => handleToggleSchedule(schedule.id, e.target.checked)}
                      />
                      <IconButton
                        aria-label="Delete schedule"
                        icon={<Trash2 size={14} />}
                        size="xs"
                        variant="ghost"
                        colorScheme="red"
                        onClick={() => handleDeleteSchedule(schedule.id)}
                      />
                    </HStack>
                  </HStack>
                ))}
              </VStack>
            </Box>
          )}

          <Stack spacing={5}>
            {/* Recurrence Type */}
            <FormControl>
              <FormLabel fontSize="sm">Repeat</FormLabel>
              <RadioGroup
                value={recurrenceType}
                onChange={(value) => setRecurrenceType(value as 'daily' | 'weekdays' | 'weekly')}
              >
                <Stack spacing={2}>
                  <Radio value="daily" size="sm">Daily</Radio>
                  <Radio value="weekdays" size="sm">Weekdays (Mon-Fri)</Radio>
                  <Radio value="weekly" size="sm">Weekly (select days)</Radio>
                </Stack>
              </RadioGroup>
            </FormControl>

            {/* Day Selection for Weekly */}
            {recurrenceType === 'weekly' && (
              <FormControl>
                <FormLabel fontSize="sm">Select Days</FormLabel>
                <HStack spacing={1} flexWrap="wrap">
                  {DAYS_OF_WEEK.map(day => (
                    <Button
                      key={day.value}
                      size="xs"
                      variant={selectedDays.includes(day.value) ? 'solid' : 'outline'}
                      colorScheme={selectedDays.includes(day.value) ? 'blue' : 'gray'}
                      onClick={() => handleDayToggle(day.value)}
                      minW="36px"
                    >
                      {day.label}
                    </Button>
                  ))}
                </HStack>
              </FormControl>
            )}

            {/* Hour Selection */}
            <FormControl>
              <FormLabel fontSize="sm">
                <HStack spacing={2}>
                  <Clock size={14} />
                  <span>Run at</span>
                </HStack>
              </FormLabel>
              <Select
                size="sm"
                value={executionHour}
                onChange={(e) => setExecutionHour(parseInt(e.target.value))}
              >
                {HOURS.map(hour => (
                  <option key={hour.value} value={hour.value}>
                    {hour.label}
                  </option>
                ))}
              </Select>
              <ChakraText fontSize="xs" color="gray.500" mt={1}>
                Timezone: {Intl.DateTimeFormat().resolvedOptions().timeZone}
              </ChakraText>
            </FormControl>

            {/* Run Mode — only shown when opened from an execution run */}
            {executionRunId && (
              <FormControl>
                <FormLabel fontSize="sm">Run mode</FormLabel>
                <RadioGroup
                  value={useContinuationMode ? 'continue' : 'fresh'}
                  onChange={(value) => setUseContinuationMode(value === 'continue')}
                >
                  <Stack spacing={2}>
                    <Radio value="continue" size="sm">
                      <HStack spacing={2}>
                        <GitBranch size={12} />
                        <span>Continue from this run</span>
                      </HStack>
                    </Radio>
                    <Radio value="fresh" size="sm">
                      <HStack spacing={2}>
                        <RefreshCw size={12} />
                        <span>Run fresh each time</span>
                      </HStack>
                    </Radio>
                  </Stack>
                </RadioGroup>
                <Box mt={1}>
                  <ChakraText fontSize="xs" color="gray.500">
                    {useContinuationMode
                      ? 'Task will remember context from this run and continue where it left off'
                      : 'Task will start from scratch each time it runs'}
                  </ChakraText>
                </Box>
              </FormControl>
            )}

            {/* Continuation instructions — only shown in continuation mode */}
            {executionRunId && useContinuationMode && (
              <FormControl>
                <FormLabel fontSize="sm">Continuation instructions (optional)</FormLabel>
                <Textarea
                  size="sm"
                  placeholder="e.g. Check for new updates and email me a summary"
                  value={continuationPrompt}
                  onChange={(e) => setContinuationPrompt(e.target.value)}
                  rows={3}
                  resize="vertical"
                />
                <Box mt={1}>
                  <ChakraText fontSize="xs" color="gray.500">
                    {continuationPrompt.trim()
                      ? 'Your instructions will be used instead of the default "Continue this run" prompt'
                      : 'Leave empty to use the default "Continue this run" prompt'}
                  </ChakraText>
                </Box>
              </FormControl>
            )}
          </Stack>
        </ModalBody>

        <ModalFooter>
          <Button variant="ghost" mr={3} onClick={onClose} size="sm">
            Close
          </Button>
          <Button
            colorScheme="blue"
            onClick={handleSubmit}
            isLoading={isSubmitting}
            loadingText="Scheduling..."
            size="sm"
          >
            Add Schedule
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
};

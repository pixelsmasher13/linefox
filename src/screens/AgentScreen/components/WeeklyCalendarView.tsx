import React, { useState, useEffect, useMemo, useCallback } from 'react';
import {
  Box,
  VStack,
  HStack,
  Text,
  IconButton,
  Heading,
  Tooltip,
  useColorModeValue,
  useBreakpointValue,
  Flex,
  Badge,
  Popover,
  PopoverTrigger,
  PopoverContent,
  PopoverBody,
  PopoverArrow,
  Button,
  useToast,
} from '@chakra-ui/react';
import {
  Calendar as CalendarIcon,
  ChevronLeft,
  ChevronRight,
  Clock,
  GitBranch,
  Edit2,
  CheckCircle2,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface ScheduleWithName {
  id: number;
  automation_id: number;
  automation_name: string;
  recurrence_type: string;
  recurrence_days: number[] | null;
  execution_hour: number;
  execution_minute: number;
  timezone: string;
  is_active: boolean;
  next_run_at: string | null;
  last_run_at: string | null;
  persistent_run_id: number | null;
  continuation_prompt: string | null;
}

interface WeeklyCalendarViewProps {
  onEditSchedule: (automationId: number, automationName: string) => void;
}

const DAYS_OF_WEEK = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

const formatHour = (h: number, m: number = 0) => {
  const minStr = m === 0 ? '' : `:${m.toString().padStart(2, '0')}`;
  if (h === 0) return `12${minStr} AM`;
  if (h < 12) return `${h}${minStr} AM`;
  if (h === 12) return `12${minStr} PM`;
  return `${h - 12}${minStr} PM`;
};

interface RecentRun {
  run: {
    id: number;
    automation_id: number;
    started_at: string;
    completed_at?: string;
    status: string;
    completion_message?: string;
  };
  automation_name?: string;
}

export const WeeklyCalendarView: React.FC<WeeklyCalendarViewProps> = ({ onEditSchedule }) => {
  const [schedules, setSchedules] = useState<ScheduleWithName[]>([]);
  const [recentRuns, setRecentRuns] = useState<RecentRun[]>([]);
  const [weekOffset, setWeekOffset] = useState(0);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const toast = useToast();

  const fetchSchedules = useCallback(async () => {
    try {
      const result = await invoke<ScheduleWithName[]>('get_all_schedules');
      setSchedules(result || []);
    } catch (err) {
      console.error('Failed to load schedules:', err);
    }
  }, []);

  const fetchRecentRuns = useCallback(async () => {
    try {
      const result = await invoke<RecentRun[]>('get_automation_execution_history', { limit: 8 });
      setRecentRuns(result || []);
    } catch (err) {
      console.error('Failed to load recent runs:', err);
    }
  }, []);

  useEffect(() => {
    fetchSchedules();
    fetchRecentRuns();
    const unlistenSchedule = listen('schedule_changed', () => {
      fetchSchedules();
      fetchRecentRuns();
    });
    const unlistenCompletion = listen('automation_completion_message', () => fetchRecentRuns());
    return () => {
      unlistenSchedule.then((fn: any) => fn());
      unlistenCompletion.then((fn: any) => fn());
    };
  }, [fetchSchedules, fetchRecentRuns]);

  const handleDeleteSchedule = async (schedule: ScheduleWithName) => {
    setDeletingId(schedule.id);
    try {
      await invoke('delete_schedule', { scheduleId: schedule.id });
      toast({ title: 'Schedule removed', status: 'success', duration: 2000 });
    } catch (error) {
      console.error('Error deleting schedule:', error);
      toast({ title: 'Failed to remove schedule', status: 'error', duration: 3000 });
    } finally {
      setDeletingId(null);
    }
  };

  const borderColor = useColorModeValue('gray.200', 'gray.600');
  const headerBg = useColorModeValue('gray.50', 'gray.700');
  const todayBg = useColorModeValue('blue.50', 'blue.900');
  const taskBg = useColorModeValue('white', 'gray.700');
  const taskHoverBg = useColorModeValue('gray.50', 'gray.600');

  const isMobile = useBreakpointValue({ base: true, md: false });
  const popoverPlacement = useBreakpointValue({ base: 'bottom', md: 'right' }) as 'bottom' | 'right';

  const weekDates = useMemo(() => {
    const today = new Date();
    const startOfWeek = new Date(today);
    startOfWeek.setDate(today.getDate() - today.getDay() + (weekOffset * 7));
    return Array.from({ length: 7 }, (_, i) => {
      const date = new Date(startOfWeek);
      date.setDate(startOfWeek.getDate() + i);
      return date;
    });
  }, [weekOffset]);

  const isToday = (date: Date) => {
    const today = new Date();
    return date.toDateString() === today.toDateString();
  };

  const getSchedulesForDay = (dayIndex: number) => {
    return schedules.filter(schedule => {
      if (!schedule.is_active) return false;
      if (schedule.recurrence_type === 'daily') return true;
      if (schedule.recurrence_type === 'weekdays') return dayIndex >= 1 && dayIndex <= 5;
      if (schedule.recurrence_type === 'weekly' && schedule.recurrence_days) {
        return schedule.recurrence_days.includes(dayIndex);
      }
      return false;
    }).sort((a, b) => {
      const hourA = a.execution_hour + (a.execution_minute || 0) / 60;
      const hourB = b.execution_hour + (b.execution_minute || 0) / 60;
      return hourA - hourB;
    });
  };

  const formatWeekRange = () => {
    const start = weekDates[0];
    const end = weekDates[6];
    const startMonth = start.toLocaleDateString('en-US', { month: 'short' });
    const endMonth = end.toLocaleDateString('en-US', { month: 'short' });
    if (startMonth === endMonth) {
      return `${startMonth} ${start.getDate()} - ${end.getDate()}, ${end.getFullYear()}`;
    }
    return `${startMonth} ${start.getDate()} - ${endMonth} ${end.getDate()}, ${end.getFullYear()}`;
  };

  const upcomingSchedules = useMemo(() => {
    return schedules
      .filter((s) => s.is_active && s.next_run_at)
      .sort((a, b) => new Date(a.next_run_at!).getTime() - new Date(b.next_run_at!).getTime())
      .slice(0, 4);
  }, [schedules]);

  const justFinishedRuns = useMemo(() => {
    return recentRuns
      .filter((r) => r.run.completed_at)
      .slice(0, 4);
  }, [recentRuns]);

  const formatRelative = (iso: string, future: boolean) => {
    const now = Date.now();
    const t = new Date(iso).getTime();
    const diffMs = future ? t - now : now - t;
    const diffMin = Math.round(diffMs / 60000);
    if (future && diffMin < 0) return 'now';
    if (!future && diffMin < 1) return 'just now';
    if (diffMin < 60) return `${future ? 'in ' : ''}${diffMin}m${future ? '' : ' ago'}`;
    const diffHr = Math.round(diffMin / 60);
    if (diffHr < 24) return `${future ? 'in ' : ''}${diffHr}h${future ? '' : ' ago'}`;
    const diffDay = Math.round(diffHr / 24);
    return `${future ? 'in ' : ''}${diffDay}d${future ? '' : ' ago'}`;
  };

  const statusColor = (status: string) => {
    if (status === 'completed' || status === 'success') return 'green.500';
    if (status === 'failed' || status === 'error') return 'red.500';
    if (status === 'stopped') return 'gray.400';
    return 'blue.500';
  };

  const getRecurrenceLabel = (schedule: ScheduleWithName) => {
    if (schedule.recurrence_type === 'daily') return 'Daily';
    if (schedule.recurrence_type === 'weekdays') return 'Weekdays';
    if (schedule.recurrence_type === 'weekly' && schedule.recurrence_days) {
      return schedule.recurrence_days.map(d => DAYS_OF_WEEK[d]).join(', ');
    }
    return '';
  };

  return (
    <Box h="full" w="full" overflow="hidden" display="flex" flexDirection="column">
      <Box p={{ base: 2, md: 4 }} borderBottomWidth="1px" borderColor={borderColor}>
        <Flex justify="space-between" align="center" wrap="wrap" gap={2}>
          <HStack spacing={2}>
            <CalendarIcon size={isMobile ? 16 : 20} />
            <Heading size={{ base: 'sm', md: 'md' }}>Schedule</Heading>
          </HStack>
          <HStack spacing={{ base: 0, md: 2 }}>
            <IconButton
              aria-label="Previous week"
              icon={<ChevronLeft size={isMobile ? 16 : 18} />}
              size="sm"
              variant="ghost"
              onClick={() => setWeekOffset(o => o - 1)}
            />
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setWeekOffset(0)}
              fontWeight={weekOffset === 0 ? 'bold' : 'normal'}
              px={{ base: 1, md: 3 }}
              fontSize={{ base: 'xs', md: 'sm' }}
            >
              {weekOffset === 0 ? 'This Week' : (isMobile ? formatWeekRange().split(',')[0] : formatWeekRange())}
            </Button>
            <IconButton
              aria-label="Next week"
              icon={<ChevronRight size={isMobile ? 16 : 18} />}
              size="sm"
              variant="ghost"
              onClick={() => setWeekOffset(o => o + 1)}
            />
          </HStack>
        </Flex>
      </Box>

      <Box flex={1} overflow="auto" p={{ base: 2, md: 4 }}>
        {(upcomingSchedules.length > 0 || justFinishedRuns.length > 0) && (
          <Flex
            direction={{ base: 'column', md: 'row' }}
            gap={{ base: 3, md: 4 }}
            mb={{ base: 3, md: 4 }}
            align="stretch"
          >
            {/* Up next */}
            <Box
              flex={1}
              borderWidth="1px"
              borderColor={borderColor}
              borderRadius="lg"
              overflow="hidden"
              bg={taskBg}
            >
              <Flex
                px={3}
                py={2}
                align="center"
                gap={2}
                bg={headerBg}
                borderBottomWidth="1px"
                borderColor={borderColor}
              >
                <Box color="blue.500">
                  <Clock size={13} />
                </Box>
                <Text
                  fontSize="2xs"
                  fontWeight="bold"
                  color="gray.500"
                  textTransform="uppercase"
                  letterSpacing="wider"
                >
                  Up next
                </Text>
              </Flex>
              {upcomingSchedules.length === 0 ? (
                <Box px={3} py={3}>
                  <Text fontSize="xs" color="gray.400">
                    Nothing scheduled.
                  </Text>
                </Box>
              ) : (
                <VStack align="stretch" spacing={0} divider={undefined as any}>
                  {upcomingSchedules.map((s, idx) => (
                    <Flex
                      key={s.id}
                      align="center"
                      gap={2.5}
                      px={3}
                      py={2}
                      borderTopWidth={idx === 0 ? 0 : '1px'}
                      borderColor={borderColor}
                    >
                      <Box w="6px" h="6px" bg="gray.300" borderRadius="full" flexShrink={0} />
                      <Box flex={1} minW={0}>
                        <Text fontSize="xs" fontWeight="semibold" color="gray.800" noOfLines={1}>
                          {s.automation_name || `Task #${s.automation_id}`}
                        </Text>
                        <Text fontSize="2xs" color="gray.500" noOfLines={1}>
                          {getRecurrenceLabel(s)} · {formatHour(s.execution_hour, s.execution_minute)}
                        </Text>
                      </Box>
                      {s.next_run_at && (
                        <Text fontSize="2xs" fontWeight="medium" color="gray.500" flexShrink={0}>
                          {formatRelative(s.next_run_at, true)}
                        </Text>
                      )}
                    </Flex>
                  ))}
                </VStack>
              )}
            </Box>

            {/* Just finished */}
            <Box
              flex={1}
              borderWidth="1px"
              borderColor={borderColor}
              borderRadius="lg"
              overflow="hidden"
              bg={taskBg}
            >
              <Flex
                px={3}
                py={2}
                align="center"
                gap={2}
                bg={headerBg}
                borderBottomWidth="1px"
                borderColor={borderColor}
              >
                <Box color="green.500">
                  <CheckCircle2 size={13} />
                </Box>
                <Text
                  fontSize="2xs"
                  fontWeight="bold"
                  color="gray.500"
                  textTransform="uppercase"
                  letterSpacing="wider"
                >
                  Just finished
                </Text>
              </Flex>
              {justFinishedRuns.length === 0 ? (
                <Box px={3} py={3}>
                  <Text fontSize="xs" color="gray.400">
                    No recent runs.
                  </Text>
                </Box>
              ) : (
                <VStack align="stretch" spacing={0}>
                  {justFinishedRuns.map((r, idx) => (
                    <Flex
                      key={r.run.id}
                      align="center"
                      gap={2.5}
                      px={3}
                      py={2}
                      borderTopWidth={idx === 0 ? 0 : '1px'}
                      borderColor={borderColor}
                    >
                      <Box
                        w="6px"
                        h="6px"
                        bg={statusColor(r.run.status)}
                        borderRadius="full"
                        flexShrink={0}
                      />
                      <Box flex={1} minW={0}>
                        <Text fontSize="xs" fontWeight="semibold" color="gray.800" noOfLines={1}>
                          {r.automation_name || `Task #${r.run.automation_id}`}
                        </Text>
                        {r.run.completion_message && (
                          <Text fontSize="2xs" color="gray.500" noOfLines={1}>
                            {r.run.completion_message}
                          </Text>
                        )}
                      </Box>
                      {r.run.completed_at && (
                        <Text fontSize="2xs" fontWeight="medium" color="gray.500" flexShrink={0}>
                          {formatRelative(r.run.completed_at, false)}
                        </Text>
                      )}
                    </Flex>
                  ))}
                </VStack>
              )}
            </Box>
          </Flex>
        )}

        <Box
          display="grid"
          gridTemplateColumns={{ base: 'repeat(7, minmax(100px, 1fr))', md: 'repeat(7, 1fr)' }}
          gap={{ base: 1, md: 2 }}
          minH={{ base: '300px', md: '400px' }}
          minW={{ base: '700px', md: 'auto' }}
        >
          {weekDates.map((date, dayIndex) => {
            const daySchedules = getSchedulesForDay(dayIndex);
            const today = isToday(date);
            return (
              <Box
                key={dayIndex}
                borderWidth="1px"
                borderColor={today ? 'blue.400' : borderColor}
                borderRadius="lg"
                overflow="hidden"
                bg={today ? todayBg : 'transparent'}
              >
                <Box
                  p={{ base: 1, md: 2 }}
                  bg={today ? 'blue.100' : headerBg}
                  borderBottomWidth="1px"
                  borderColor={borderColor}
                  textAlign="center"
                >
                  <Text
                    fontSize={{ base: '2xs', md: 'xs' }}
                    fontWeight="medium"
                    color={today ? 'blue.600' : 'gray.500'}
                    textTransform="uppercase"
                  >
                    {isMobile ? DAYS_OF_WEEK[dayIndex].charAt(0) : DAYS_OF_WEEK[dayIndex]}
                  </Text>
                  <Text
                    fontSize={{ base: 'sm', md: 'lg' }}
                    fontWeight={today ? 'bold' : 'medium'}
                    color={today ? 'blue.600' : 'gray.700'}
                  >
                    {date.getDate()}
                  </Text>
                </Box>

                <VStack spacing={1} p={{ base: 1, md: 2 }} align="stretch" minH={{ base: '80px', md: '120px' }}>
                  {daySchedules.length === 0 ? (
                    <Text fontSize={{ base: '2xs', md: 'xs' }} color="gray.400" textAlign="center" mt={{ base: 2, md: 4 }}>
                      {isMobile ? '-' : 'No tasks'}
                    </Text>
                  ) : (
                    daySchedules.map((schedule) => (
                      <Popover key={schedule.id} placement={popoverPlacement} isLazy>
                        <PopoverTrigger>
                          <Box
                            p={{ base: 1, md: 2 }}
                            bg={taskBg}
                            borderRadius="md"
                            borderWidth="1px"
                            borderColor={borderColor}
                            cursor="pointer"
                            _hover={{ bg: taskHoverBg, borderColor: 'blue.300' }}
                            transition="all 0.15s"
                          >
                            <Text
                              fontSize={{ base: '2xs', md: 'xs' }}
                              fontWeight="medium"
                              noOfLines={isMobile ? 1 : 2}
                              color="gray.700"
                            >
                              {schedule.automation_name || `Task #${schedule.automation_id}`}
                            </Text>
                            <HStack spacing={1} mt={1}>
                              <Clock size={isMobile ? 8 : 10} color="gray" />
                              <Text fontSize={{ base: '2xs', md: 'xs' }} color="gray.500">
                                {formatHour(schedule.execution_hour, schedule.execution_minute)}
                              </Text>
                              {schedule.persistent_run_id && !isMobile && (
                                <Tooltip label="Continuation mode">
                                  <Box as="span" color="purple.500">
                                    <GitBranch size={10} />
                                  </Box>
                                </Tooltip>
                              )}
                            </HStack>
                          </Box>
                        </PopoverTrigger>
                        <PopoverContent w="260px">
                          <PopoverArrow />
                          <PopoverBody p={3}>
                            <VStack spacing={2} align="stretch">
                              <Box>
                                <Text fontWeight="semibold" fontSize="sm">
                                  {schedule.automation_name || `Task #${schedule.automation_id}`}
                                </Text>
                                <HStack spacing={2} mt={1}>
                                  <Badge colorScheme="blue">
                                    {getRecurrenceLabel(schedule)}
                                  </Badge>
                                  <Badge colorScheme="gray">
                                    {formatHour(schedule.execution_hour, schedule.execution_minute)}
                                  </Badge>
                                </HStack>
                              </Box>

                              {schedule.persistent_run_id && (
                                <Box fontSize="xs" color="purple.600" bg="purple.50" p={2} borderRadius="md">
                                  <HStack spacing={1}>
                                    <GitBranch size={12} />
                                    <Text fontWeight="medium">Continuation mode</Text>
                                  </HStack>
                                  {schedule.continuation_prompt && (
                                    <Text mt={1} color="gray.600" noOfLines={2}>
                                      "{schedule.continuation_prompt}"
                                    </Text>
                                  )}
                                </Box>
                              )}

                              {schedule.next_run_at && (
                                <Text fontSize="xs" color="gray.500">
                                  Next: {new Date(schedule.next_run_at).toLocaleString()}
                                </Text>
                              )}

                              <VStack spacing={1} w="full">
                                <Button
                                  size="sm"
                                  colorScheme="blue"
                                  variant="outline"
                                  leftIcon={<Edit2 size={14} />}
                                  onClick={() => onEditSchedule(schedule.automation_id, schedule.automation_name)}
                                  w="full"
                                >
                                  Edit Schedule
                                </Button>
                                <Button
                                  size="sm"
                                  colorScheme="red"
                                  variant="ghost"
                                  onClick={() => handleDeleteSchedule(schedule)}
                                  isLoading={deletingId === schedule.id}
                                  w="full"
                                >
                                  Remove
                                </Button>
                              </VStack>
                            </VStack>
                          </PopoverBody>
                        </PopoverContent>
                      </Popover>
                    ))
                  )}
                </VStack>
              </Box>
            );
          })}
        </Box>

        {schedules.length === 0 && (
          <Flex
            direction="column"
            align="center"
            justify="center"
            h={{ base: '150px', md: '200px' }}
            color="gray.500"
            mt={{ base: 4, md: 8 }}
          >
            <CalendarIcon size={isMobile ? 36 : 48} strokeWidth={1} />
            <Text mt={{ base: 2, md: 4 }} fontSize={{ base: 'md', md: 'lg' }} fontWeight="medium">
              No scheduled tasks
            </Text>
            <Text fontSize={{ base: 'xs', md: 'sm' }} mt={1} textAlign="center" px={4}>
              Schedule a task to see it appear here
            </Text>
          </Flex>
        )}
      </Box>
    </Box>
  );
};

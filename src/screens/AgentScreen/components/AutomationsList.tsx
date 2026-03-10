import React, { useEffect, useRef } from 'react';
import { VStack, HStack, Box, Menu, MenuButton, MenuList, MenuItem, IconButton, Flex } from '@chakra-ui/react';
import { Text } from '@heelix-app/design';
import { Trash2, MoreHorizontal, Zap, Calendar } from 'lucide-react';
import { Automation } from '../types';

interface AutomationsListProps {
  automations: Automation[];
  selectedAutomationId: number | null;
  onSelectAutomation: (id: number) => void;
  onDeleteAutomation: (id: number) => void;
  onScheduleAutomation?: (id: number, name: string) => void;
  onShareAutomation?: (id: number) => void;
  isHistory?: boolean;
  isActive?: boolean;
}

export const AutomationsList: React.FC<AutomationsListProps> = ({
  automations,
  selectedAutomationId,
  onSelectAutomation,
  onDeleteAutomation,
  onScheduleAutomation,
  isHistory = false,
  isActive = true,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (!automations.length || !isActive) return;
      
      const currentIndex = automations.findIndex(a => a.id === selectedAutomationId);
      let newIndex = currentIndex;
      
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        newIndex = currentIndex < automations.length - 1 ? currentIndex + 1 : 0;
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        newIndex = currentIndex > 0 ? currentIndex - 1 : automations.length - 1;
      } else {
        return;
      }
      
      if (newIndex !== currentIndex && automations[newIndex]) {
        onSelectAutomation(automations[newIndex].id);
        
        setTimeout(() => {
          const selectedElement = containerRef.current?.querySelector(`[data-automation-id="${automations[newIndex].id}"]`) as HTMLElement;
          selectedElement?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }, 0);
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [automations, selectedAutomationId, onSelectAutomation, isActive]);

  const handleDelete = (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    onDeleteAutomation(id);
  };

  const truncateName = (name: string, maxLength: number = 28) => {
    if (name.length <= maxLength) return name;
    return name.substring(0, maxLength) + "...";
  };

  if (automations.length === 0) {
    return (
      <Flex justify="center" align="center" py={8}>
        <Text type="s" secondary>
          {isHistory 
            ? 'No task history yet' 
            : 'No tasks yet. Create one above!'}
        </Text>
      </Flex>
    );
  }

  return (
    <VStack ref={containerRef} spacing={0} align="stretch" width="100%">
      {automations.map((automation) => (
        <HStack
          key={automation.id}
          data-automation-id={automation.id}
          tabIndex={0}
          px={3}
          py={2}
          spacing={2}
          borderRadius="6px"
          bg={selectedAutomationId === automation.id ? "rgba(37, 99, 235, 0.1)" : "transparent"}
          cursor="pointer"
          transition="all 0.1s ease"
          outline="none"
          _hover={{ bg: selectedAutomationId === automation.id ? "rgba(37, 99, 235, 0.15)" : "rgba(0, 0, 0, 0.04)" }}
          _focus={{ outline: "none" }}
          onClick={() => onSelectAutomation(automation.id)}
        >
          <Zap size={14} color="#9ca3af" />
          <Box flex={1}>
            <Text type="s">
              {truncateName(automation.name)}
            </Text>
          </Box>
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
            <MenuList minW="140px" py={1}>
              {onScheduleAutomation && (
                <MenuItem
                  icon={<Calendar size={14} />}
                  onClick={(e: React.MouseEvent) => {
                    e.stopPropagation();
                    onScheduleAutomation(automation.id, automation.name);
                  }}
                  fontSize="sm"
                >
                  Schedule
                </MenuItem>
              )}
              <MenuItem
                icon={<Trash2 size={14} />}
                onClick={(e: React.MouseEvent) => handleDelete(e, automation.id)}
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

import React, { useState, useEffect } from 'react';
import {
  Box,
  VStack,
  HStack,
  Button,
  Text,
  IconButton,
  Flex,
  Badge,
  useToast,
  Spinner,
  Tooltip,
} from '@chakra-ui/react';
import { Plus, Edit2, Trash2, ToggleLeft, ToggleRight, Zap } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface Skill {
  id: number;
  name: string;
  skill_type: string;
  domains: string[];
  triggers: string[];
  description: string;
  content: string;
  is_active: boolean;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

interface SkillsViewProps {
  onSelectSkill?: (skill: Skill) => void;
  refreshTrigger?: number; // Increment to trigger refresh from parent
}

export const SkillsView: React.FC<SkillsViewProps> = ({ onSelectSkill, refreshTrigger }) => {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const toast = useToast();

  const fetchSkills = async () => {
    try {
      setIsLoading(true);
      const result = await invoke<Skill[]>('get_all_skills');
      setSkills(result);
    } catch (error) {
      console.error('Failed to fetch skills:', error);
      toast({
        title: 'Error loading skills',
        description: String(error),
        status: 'error',
        duration: 3000,
      });
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchSkills();
  }, [refreshTrigger]);
  
  // Listen for skill_changed events from backend
  useEffect(() => {
    const unlisten = listen('skill_changed', () => {
      fetchSkills();
    });
    
    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  const handleToggleActive = async (e: React.MouseEvent, skill: Skill) => {
    e.stopPropagation();
    try {
      await invoke('toggle_skill_active', {
        skillId: skill.id,
        isActive: !skill.is_active,
      });
      setSkills(skills.map(s => 
        s.id === skill.id ? { ...s, is_active: !s.is_active } : s
      ));
      toast({
        title: skill.is_active ? 'Skill disabled' : 'Skill enabled',
        status: 'success',
        duration: 2000,
      });
    } catch (error) {
      console.error('Failed to toggle skill:', error);
      toast({
        title: 'Error',
        description: String(error),
        status: 'error',
        duration: 3000,
      });
    }
  };

  const handleDelete = async (e: React.MouseEvent, skill: Skill) => {
    e.stopPropagation();
    if (skill.is_default) {
      toast({
        title: 'Cannot delete default skill',
        description: 'You can only disable default skills',
        status: 'warning',
        duration: 3000,
      });
      return;
    }

    try {
      await invoke('delete_skill', { skillId: skill.id });
      setSkills(skills.filter(s => s.id !== skill.id));
      toast({
        title: 'Skill deleted',
        status: 'success',
        duration: 2000,
      });
    } catch (error) {
      console.error('Failed to delete skill:', error);
      toast({
        title: 'Error',
        description: String(error),
        status: 'error',
        duration: 3000,
      });
    }
  };

  const handleCreateNew = (type: 'site' | 'role' = 'site') => {
    const isRole = type === 'role';
    const newSkill: Skill = {
      id: 0,
      name: isRole ? 'New Role' : 'New Skill',
      skill_type: type,
      domains: [],
      triggers: [],
      description: '',
      content: isRole ? `# Role Instructions

Describe how the AI should behave when this role is active.

## Focus Areas
- What should the agent prioritize?

## Approach
- How should it handle tasks differently?` : `# My Skill

## Navigation Tips
- How to navigate this website or app effectively

## Shortcuts & Features
- Useful keyboard shortcuts or hidden features

## Best Practices
- Tips for accomplishing common tasks`,
      is_active: true,
      is_default: false,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    if (onSelectSkill) {
      onSelectSkill(newSkill);
    }
  };

  const handleSelectSkill = (skill: Skill) => {
    if (onSelectSkill) {
      onSelectSkill(skill);
    }
  };

  return (
    <Box h="100%" display="flex" flexDirection="column">
      {/* Skills List */}
      {isLoading ? (
        <Flex justify="center" py={8}>
          <Spinner size="sm" />
        </Flex>
      ) : skills.length === 0 ? (
        <Flex
          direction="column"
          align="center"
          justify="center"
          py={6}
          color="gray.500"
        >
          <Text fontSize="sm" mb={2}>No skills yet</Text>
          <Button size="xs" onClick={() => handleCreateNew('site')}>
            Create your first skill
          </Button>
        </Flex>
      ) : (
        <VStack spacing={1} align="stretch" flex={1} overflowY="auto">
          {/* Roles section */}
          {skills.some(s => s.skill_type === 'role') && (
            <>
              <HStack px={3} pt={1} pb={1} justify="space-between" align="center">
                <Text fontSize="xs" color="gray.400" fontWeight="medium" textTransform="uppercase" letterSpacing="wider">
                  Roles
                </Text>
                <Box as="button" fontSize="xs" color="blue.500" _hover={{ color: 'blue.600' }} onClick={() => handleCreateNew('role')}>
                  + New Role
                </Box>
              </HStack>
              {skills.filter(s => s.skill_type === 'role').map((skill) => (
                <HStack key={skill.id} px={3} py={2} spacing={2} borderRadius="6px" cursor="pointer" transition="all 0.1s ease" opacity={skill.is_active ? 1 : 0.5} _hover={{ bg: "rgba(0, 0, 0, 0.04)" }} onClick={() => handleSelectSkill(skill)}>
                  <Zap size={14} color={skill.is_active ? "#8b5cf6" : "#9ca3af"} style={{ flexShrink: 0, marginTop: 2 }} />
                  <Box flex={1} minW={0}>
                    <Text fontSize="sm" fontWeight="medium" noOfLines={1}>{skill.name}</Text>
                    {skill.description && <Text fontSize="xs" color="gray.500" noOfLines={1}>{skill.description}</Text>}
                  </Box>
                  <HStack spacing={0}>
                    <Tooltip label={skill.is_active ? 'Disable' : 'Enable'}>
                      <IconButton aria-label="Toggle" icon={skill.is_active ? <ToggleRight size={12} /> : <ToggleLeft size={12} />} size="xs" variant="ghost" colorScheme={skill.is_active ? 'green' : 'gray'} onClick={(e) => handleToggleActive(e, skill)} />
                    </Tooltip>
                    {!skill.is_default && (
                      <Tooltip label="Delete">
                        <IconButton aria-label="Delete" icon={<Trash2 size={12} />} size="xs" variant="ghost" colorScheme="red" onClick={(e) => handleDelete(e, skill)} />
                      </Tooltip>
                    )}
                  </HStack>
                </HStack>
              ))}
              <Box h="1px" bg="gray.100" mx={3} my={2} />
            </>
          )}

          {/* Skills section */}
          <HStack px={3} pt={1} pb={1} justify="space-between" align="center">
            <Text fontSize="xs" color="gray.400" fontWeight="medium" textTransform="uppercase" letterSpacing="wider">
              Skills
            </Text>
            <Box as="button" fontSize="xs" color="blue.500" _hover={{ color: 'blue.600' }} onClick={() => handleCreateNew('site')}>
              + New Skill
            </Box>
          </HStack>
          {skills.filter(s => s.skill_type !== 'role').map((skill) => (
            <HStack key={skill.id} px={3} py={2} spacing={2} borderRadius="6px" cursor="pointer" transition="all 0.1s ease" opacity={skill.is_active ? 1 : 0.5} _hover={{ bg: "rgba(0, 0, 0, 0.04)" }} onClick={() => handleSelectSkill(skill)}>
              <Zap size={14} color={skill.is_active ? "#3b82f6" : "#9ca3af"} style={{ flexShrink: 0, marginTop: 2 }} />
              <Box flex={1} minW={0}>
                <Text fontSize="sm" fontWeight="medium" noOfLines={1}>{skill.name}</Text>
                {skill.description && <Text fontSize="xs" color="gray.500" noOfLines={1}>{skill.description}</Text>}
              </Box>
              <HStack spacing={0}>
                <Tooltip label={skill.is_active ? 'Disable' : 'Enable'}>
                  <IconButton aria-label="Toggle" icon={skill.is_active ? <ToggleRight size={12} /> : <ToggleLeft size={12} />} size="xs" variant="ghost" colorScheme={skill.is_active ? 'green' : 'gray'} onClick={(e) => handleToggleActive(e, skill)} />
                </Tooltip>
                {!skill.is_default && (
                  <Tooltip label="Delete">
                    <IconButton aria-label="Delete" icon={<Trash2 size={12} />} size="xs" variant="ghost" colorScheme="red" onClick={(e) => handleDelete(e, skill)} />
                  </Tooltip>
                )}
              </HStack>
            </HStack>
          ))}
        </VStack>
      )}
    </Box>
  );
};

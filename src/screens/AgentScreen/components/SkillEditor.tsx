import React, { useState, useEffect } from 'react';
import {
  Box,
  VStack,
  HStack,
  Button,
  Input,
  Textarea,
  FormControl,
  FormLabel,
  FormHelperText,
  Select,
  IconButton,
  Flex,
  Tag,
  TagLabel,
  TagCloseButton,
  useToast,
} from '@chakra-ui/react';
import { ArrowLeft, Plus } from 'lucide-react';
import type { Skill } from './SkillsView';

interface SkillEditorProps {
  skill: Skill;
  onSave: (skill: Partial<Skill>) => void;
  onCancel: () => void;
}

export const SkillEditor: React.FC<SkillEditorProps> = ({
  skill,
  onSave,
  onCancel,
}) => {
  const [name, setName] = useState(skill.name);
  const [skillType, setSkillType] = useState(skill.skill_type === 'persona' ? 'site' : skill.skill_type);
  const isRole = skillType === 'role';
  const [description, setDescription] = useState(skill.description || '');
  const [domains, setDomains] = useState<string[]>(skill.domains || []);
  const [content, setContent] = useState(skill.content);
  const [newDomain, setNewDomain] = useState('');
  const toast = useToast();

  // Reset internal state when skill prop changes (e.g., selecting a different skill)
  useEffect(() => {
    setName(skill.name);
    setSkillType(skill.skill_type === 'persona' ? 'site' : (skill.skill_type || 'site'));
    setDescription(skill.description || '');
    setDomains(skill.domains || []);
    setContent(skill.content);
    setNewDomain('');
  }, [skill.id]);

  const handleAddDomain = () => {
    if (newDomain.trim() && !domains.includes(newDomain.trim())) {
      setDomains([...domains, newDomain.trim()]);
      setNewDomain('');
    }
  };

  const handleRemoveDomain = (domain: string) => {
    setDomains(domains.filter(d => d !== domain));
  };

  const handleSave = () => {
    if (!name.trim()) {
      toast({
        title: 'Name is required',
        status: 'error',
        duration: 2000,
      });
      return;
    }

    if (!isRole && domains.length === 0) {
      const itemName = skillType === 'site' ? 'domain' : 'app name';
      toast({
        title: `At least one ${itemName} is required`,
        status: 'error',
        duration: 2000,
      });
      return;
    }

    onSave({
      id: skill.id,
      name: name.trim(),
      skill_type: skillType,
      domains,
      triggers: [], // No longer used
      description: description.trim(),
      content,
      is_active: skill.is_active,
    });
  };

  return (
    <Box h="100%" display="flex" flexDirection="column">
      {/* Header */}
      <Flex align="center" mb={4}>
        <IconButton
          aria-label="Back"
          icon={<ArrowLeft size={20} />}
          variant="ghost"
          onClick={onCancel}
          mr={2}
        />
        <Box flex={1} fontWeight="bold" fontSize="lg">
          {skill.id > 0 ? 'Edit Skill' : 'New Skill'}
        </Box>
      </Flex>

      {/* Form */}
      <VStack spacing={4} align="stretch" flex={1} overflowY="auto">
        <FormControl isRequired>
          <FormLabel fontSize="sm">Skill Name</FormLabel>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g., LinkedIn Navigation"
            size="sm"
          />
        </FormControl>

        <FormControl>
          <FormLabel fontSize="sm">Description</FormLabel>
          <Input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Short description shown in the skills list"
            size="sm"
          />
        </FormControl>

        {!isRole && (
          <FormControl isRequired>
            <FormLabel fontSize="sm">Skill Type</FormLabel>
            <Select
              value={skillType}
              onChange={(e) => {
                setSkillType(e.target.value);
                setDomains([]); // Clear domains when switching type
              }}
              size="sm"
              isDisabled={skill.is_default}
            >
              <option value="site">Website</option>
              <option value="app">App</option>
            </Select>
            <FormHelperText fontSize="xs">
              {skillType === 'site'
                ? 'Website skills are applied when visiting specific domains in a browser'
                : 'App skills are applied when using specific desktop applications'
              }
            </FormHelperText>
          </FormControl>
        )}

        {isRole && (
          <Box fontSize="xs" color="purple.500" px={1}>
            Role instructions are injected as additional context when you select this role before running a task.
          </Box>
        )}

        {!isRole && (
        <FormControl isRequired>
          <FormLabel fontSize="sm">
            {skillType === 'site' ? 'Domains' : 'App Names'}
          </FormLabel>
          <HStack mb={2}>
            <Input
              value={newDomain}
              onChange={(e) => setNewDomain(e.target.value)}
              placeholder={skillType === 'site' ? 'e.g., linkedin.com' : 'e.g., Visual Studio Code'}
              size="sm"
              onKeyPress={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  handleAddDomain();
                }
              }}
            />
            <IconButton
              aria-label={skillType === 'site' ? 'Add domain' : 'Add app name'}
              icon={<Plus size={16} />}
              size="sm"
              onClick={handleAddDomain}
            />
          </HStack>
          <Flex wrap="wrap" gap={2}>
            {domains.map((domain) => (
              <Tag key={domain} size="sm" colorScheme={skillType === 'site' ? 'blue' : 'green'}>
                <TagLabel>{domain}</TagLabel>
                <TagCloseButton onClick={() => handleRemoveDomain(domain)} />
              </Tag>
            ))}
          </Flex>
          <FormHelperText fontSize="xs">
            {skillType === 'site'
              ? 'Add domains where this skill applies (e.g., linkedin.com, github.com)'
              : 'Add app names where this skill applies (e.g., Visual Studio Code, Microsoft Excel)'
            }
          </FormHelperText>
        </FormControl>
        )}

        <FormControl isRequired flex={1} display="flex" flexDirection="column">
          <FormLabel fontSize="sm">Content</FormLabel>
          <Textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            placeholder="Write instructions or tips for the AI when this skill is active..."
            size="sm"
            flex={1}
            minH="200px"
            resize="vertical"
            fontFamily="mono"
            fontSize="xs"
          />
          <FormHelperText fontSize="xs">
            Markdown format. Provide navigation tips, best practices, or app-specific instructions.
          </FormHelperText>
        </FormControl>
      </VStack>

      {/* Actions */}
      <HStack mt={4} justify="flex-end">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button colorScheme="blue" onClick={handleSave}>
          Save Skill
        </Button>
      </HStack>
    </Box>
  );
};

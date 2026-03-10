import React, { useState, useEffect } from 'react';
import { 
  Box, 
  Flex, 
  VStack, 
  FormControl, 
  FormLabel, 
  Input, 
  Textarea, 
  Button, 
  HStack,
} from '@chakra-ui/react';
import { Text } from '@heelix-app/design';
import { Target, FileText, Sparkles } from 'lucide-react';
import { AutomationScript } from '../types';
import styled from 'styled-components';

const ScriptContainer = styled(Box)`
  width: 100%;
`;

const DescriptionContainer = styled(Box)`
  max-height: 150px;
  overflow-y: auto;
  padding: var(--space-l);
  margin-top: var(--space-m);
  margin-bottom: var(--space-m);
  background-color: var(--color-background-secondary);
  border-left: 3px solid var(--color-border);
  border-radius: 8px;
  
  &::-webkit-scrollbar {
    width: 10px;
  }
  
  &::-webkit-scrollbar-track {
    background: rgba(0, 0, 0, 0.05);
    border-radius: 4px;
  }
  
  &::-webkit-scrollbar-thumb {
    background: rgba(0, 0, 0, 0.2);
    border-radius: 4px;
  }
  
  &::-webkit-scrollbar-thumb:hover {
    background: rgba(0, 0, 0, 0.3);
  }
`;

const SectionHeader = styled(Flex)`
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  
  svg {
    color: #4299e1;
  }
`;



interface AutomationScriptEditorProps {
  script: AutomationScript;
  isEditing: boolean;
  onSave: (script: AutomationScript) => void;
  onCancel: () => void;
  onInstructionsChange?: (instructions: string) => void;
  additionalInstructions?: string;
}

export const AutomationScriptEditor: React.FC<AutomationScriptEditorProps> = ({
  script,
  isEditing,
  onSave,
  onCancel,
  onInstructionsChange,
  additionalInstructions: externalInstructions
}) => {
  const [editedScript, setEditedScript] = useState<AutomationScript>({ ...script });
  const [localInstructions, setLocalInstructions] = useState(externalInstructions || script.additional_instructions || '');

  useEffect(() => {
    if (externalInstructions !== undefined) {
      setLocalInstructions(externalInstructions);
    }
  }, [externalInstructions]);

  const handleInstructionsChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newValue = e.target.value;
    setLocalInstructions(newValue);
    if (onInstructionsChange) {
      onInstructionsChange(newValue);
    }
  };

  const handleInputChange = (
    e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>
  ) => {
    const { name, value } = e.target;
    setEditedScript(prev => ({ ...prev, [name]: value }));
  };


  const handleSubmit = () => {
    onSave({
      ...editedScript,
      goal: script.goal,
    });
  };

  return (
    <ScriptContainer>
      {isEditing ? (
        <VStack spacing={4} align="stretch">
          <FormControl>
            <FormLabel>Name</FormLabel>
            <Input
              name="name"
              value={editedScript.name}
              onChange={handleInputChange}
            />
          </FormControl>
          
          <FormControl>
            <FormLabel>Objective</FormLabel>
            <Textarea
              name="objective"
              value={editedScript.objective || ''}
              onChange={handleInputChange}
              rows={3}
              placeholder="What is the goal of this task?"
              onKeyDown={(e) => {
                // Prevent arrow keys and other navigation keys from bubbling up
                if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
                  e.stopPropagation();
                }
              }}
            />
          </FormControl>
          
          <FormControl>
            <FormLabel>Generalized Script</FormLabel>
            <Textarea
              name="nl_description"
              value={editedScript.nl_description || ''}
              onChange={handleInputChange}
              rows={6}
              placeholder="Describe in plain language what this task does..."
              onKeyDown={(e) => {
                // Prevent arrow keys and other navigation keys from bubbling up
                if (['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
                  e.stopPropagation();
                }
              }}
            />
            <Box mt={1}>
              <Text type="xs" secondary>
                This is a natural language description of what the task does.
              </Text>
            </Box>
          </FormControl>
          
          <HStack spacing={4} justify="flex-end" mt={6}>
            <Button onClick={onCancel} variant="outline">
              Cancel
            </Button>
            <Button onClick={handleSubmit} colorScheme="blue">
              Save Changes
            </Button>
          </HStack>
        </VStack>
      ) : (
        <VStack spacing={6} align="stretch">
          <Box>
            <SectionHeader>
              <Target size={18} />
              <Text type="s" bold>Objective</Text>
            </SectionHeader>
            <Box ml={6}>
              <Text type="m">{script.objective || 'No objective set'}</Text>
            </Box>
          </Box>
          
          {script.nl_description && (
            <Box>
              <SectionHeader>
                <FileText size={18} />
                <Text type="s" bold>What This Task Does</Text>
              </SectionHeader>
              <DescriptionContainer>
                <Box style={{ lineHeight: '1.6' }}>
                  <Text type="m">{script.nl_description}</Text>
                </Box>
              </DescriptionContainer>
            </Box>
          )}
          
          <Box>
            <SectionHeader>
              <Sparkles size={18} />
              <Text type="s" bold>Additional Instructions</Text>
            </SectionHeader>
            <Textarea
              value={localInstructions}
              onChange={handleInstructionsChange}
              rows={3}
              placeholder="Add any additional instructions for running this task..."
              mt={1}
            />
            <Box mt={1}>
              <Text type="xs" secondary>
                These instructions will be used when you run the task, but are not saved permanently.
              </Text>
            </Box>
          </Box>
        </VStack>
      )}
    </ScriptContainer>
  );
}; 
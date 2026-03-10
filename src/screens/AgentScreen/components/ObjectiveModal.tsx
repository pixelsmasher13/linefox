import React, { useState } from 'react';
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  Button,
  FormControl,
  FormLabel,
  Input,
  Textarea,
  VStack,
  Text,
  FormHelperText,
  Box,
  Flex,
  HStack
} from '@chakra-ui/react';
import { FileText, Target, Sparkles, CircleDot } from 'lucide-react';
import styled from 'styled-components';

interface ObjectiveModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSubmit: (objective: string, name: string) => void;
}

const StyledModalContent = styled(ModalContent)`
  border-radius: 12px;
  overflow: hidden;
`;

const InputContainer = styled(Box)`
  background-color: rgba(37, 99, 235, 0.02);
  border: 1px solid rgba(37, 99, 235, 0.1);
  border-radius: 8px;
  padding: var(--space-m);
  transition: all 0.2s ease;
  
  &:hover {
    border-color: rgba(37, 99, 235, 0.2);
    background-color: rgba(37, 99, 235, 0.03);
  }
  
  &:focus-within {
    border-color: #2563eb;
    background-color: white;
    box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.1);
  }
`;

const SectionHeader = styled(Flex)`
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
  
  svg {
    color: #2563eb;
  }
`;

export const ObjectiveModal: React.FC<ObjectiveModalProps> = ({
  isOpen,
  onClose,
  onSubmit
}) => {
  const [objective, setObjective] = useState('');
  const [name, setName] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [attemptedSubmit, setAttemptedSubmit] = useState(false);

  // Validation states
  const isNameEmpty = name.trim() === '';
  const isObjectiveEmpty = objective.trim() === '';

  const handleSubmit = () => {
    setAttemptedSubmit(true);
    if (!isNameEmpty && !isObjectiveEmpty) {
      setIsSubmitting(true);
      onSubmit(objective, name);
      // Reset form
      setObjective('');
      setName('');
      setIsSubmitting(false);
      setAttemptedSubmit(false);
    }
  };
  
  const handleClose = () => {
    setObjective('');
    setName('');
    setAttemptedSubmit(false);
    onClose();
  };

  return (
    <Modal isOpen={isOpen} onClose={handleClose} size="lg">
      <ModalOverlay backdropFilter="blur(4px)" />
      <StyledModalContent>
        <Box bg="white" p={6} borderBottom="1px solid" borderColor="gray.100">
          <HStack spacing={3} mb={2}>
            <CircleDot size={24} color="#2563eb" />
            <Text fontSize="xl" fontWeight="bold" color="gray.800">Create New Task</Text>
          </HStack>
       
        </Box>
        <ModalCloseButton />
        <ModalBody pt={6} pb={6}>
          <Text fontSize="sm" color="gray.600" mb={5}>
            Add details about your task to help Linefox AI understand what you want to accomplish. 
          </Text>
          <VStack spacing={5}>
            <FormControl isRequired>
              <SectionHeader>
                <FormLabel fontWeight="600" mb={0}>Task Name</FormLabel>
              </SectionHeader>
              <InputContainer>
                <Input 
                  border="none"
                  bg="transparent"
                  placeholder="Enter a descriptive name (e.g., 'Export Monthly Sales Report')"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  _focus={{ boxShadow: 'none' }}
                  fontSize="15px"
                />
              </InputContainer>
              <FormHelperText fontSize="xs" mt={2} color={attemptedSubmit && isNameEmpty ? "red.500" : "gray.500"}>
                {attemptedSubmit && isNameEmpty 
                  ? "⚠️ Name is required for your task"
                  : "Choose a meaningful name that describes the task"
                }
              </FormHelperText>
            </FormControl>
            <FormControl isRequired>
              <SectionHeader>
                <FormLabel fontWeight="600" mb={0}>Task Objective</FormLabel>
              </SectionHeader>
              <InputContainer>
                <Textarea
                  border="none"
                  bg="transparent"
                  placeholder="Describe what you want to accomplish with this task..."
                  value={objective}
                  onChange={(e) => setObjective(e.target.value)}
                  rows={5}
                  _focus={{ boxShadow: 'none' }}
                  fontSize="14px"
                  resize="vertical"
                />
              </InputContainer>
              <FormHelperText fontSize="xs" mt={2} color={attemptedSubmit && isObjectiveEmpty ? "red.500" : "gray.500"}>
                {attemptedSubmit && isObjectiveEmpty
                  ? "⚠️ Please provide an objective for your task"
                  : "Example: 'Log into CRM, search customer records by date, export as CSV'"
                }
              </FormHelperText>
            </FormControl>
          </VStack>
        </ModalBody>

        <ModalFooter borderTop="1px solid" borderColor="gray.100" bg="gray.50">
          <Button variant="ghost" mr={3} onClick={handleClose}>
            Cancel
          </Button>
          <Button 
            leftIcon={<CircleDot size={16} />}
            bg="#2563eb"
            color="white"
            _hover={{
              bg: "#1d4ed8",
              transform: "translateY(-1px)",
              boxShadow: "0 4px 12px rgba(37, 99, 235, 0.25)"
            }}
            _active={{
              bg: "#1e40af",
              transform: "translateY(0)"
            }}
            onClick={handleSubmit}
            isDisabled={isNameEmpty || isObjectiveEmpty}
            isLoading={isSubmitting}
            loadingText="Starting..."
            px={6}
            fontWeight="500"
          >
            Start Recording
          </Button>
        </ModalFooter>
      </StyledModalContent>
    </Modal>
  );
}; 
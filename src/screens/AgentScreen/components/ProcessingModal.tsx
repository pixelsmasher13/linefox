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
  VStack,
  Text,
  Progress,
  Box,
  Spinner,
  Flex,
  Textarea,
  useToast,
  Tabs,
  TabList,
  Tab,
  TabPanels,
  TabPanel
} from '@chakra-ui/react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { automationService } from '../../../data/automation';

interface ProcessingModalProps {
  isOpen: boolean;
  onClose: () => void;
  automationId: number | null;
}

export const ProcessingModal: React.FC<ProcessingModalProps> = ({
  isOpen,
  onClose,
  automationId
}) => {
  const [isProcessing, setIsProcessing] = useState(true);
  const [status, setStatus] = useState("Processing task events with AI...");
  const [automationScript, setAutomationScript] = useState("");
  const [editedScript, setEditedScript] = useState("");
  const [humanReadableScript, setHumanReadableScript] = useState("");
  const [isEditing, setIsEditing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [tabIndex, setTabIndex] = useState(0);
  const toast = useToast();

  // Listen for status updates
  useEffect(() => {
    if (!isOpen || !automationId) return;

    const setupListener = async () => {
      const unlisten = await listen('recording_status', (event: any) => {
        console.log("Recording status event received:", event.payload);
        
        if (event.payload.automationId === automationId) {
          setIsProcessing(event.payload.isProcessing);
          if (event.payload.status) {
            setStatus(event.payload.status);
          }
          
          // When processing is done, fetch the automation script
          if (!event.payload.isProcessing) {
            console.log("Processing complete, fetching automation script");
            fetchAutomationScript();
          }
        } else {
          console.log(`Received event for automation ${event.payload.automationId}, but current ID is ${automationId}`);
        }
      });
      
      return unlisten;
    };
    
    const unlistenPromise = setupListener();
    
    return () => {
      unlistenPromise.then(unlisten => unlisten());
    };
  }, [isOpen, automationId]);
  
  // Fetch the automation script once processing is complete
  const fetchAutomationScript = async () => {
    if (!automationId) {
      console.error("Cannot fetch script: No automation ID provided");
      return;
    }
    
    console.log(`Fetching automation script for ID: ${automationId}`);
    
    try {
      // Get the automation script (JSON)
      const script = await invoke('get_automation_script', { automationId });
      console.log("Retrieved automation script:", script);
      setAutomationScript(JSON.stringify(script, null, 2));
      setEditedScript(JSON.stringify(script, null, 2));
      
      // Get the human-readable description
      const automation = await automationService.fetchAutomationById(automationId);
      console.log("Retrieved automation:", automation);
      
      if (automation && automation.nl_description && 
          automation.nl_description !== "Human-readable summary temporarily disabled." &&
          automation.nl_description !== "Error processing task") {
        setHumanReadableScript(automation.nl_description);
        // Default to the human readable tab (index 0)
        setTabIndex(0);
      } else {
        setHumanReadableScript("Human-readable summary not available. Using the technical script instead.");
        // Default to the technical tab (index 1) since human summary is not available
        setTabIndex(1);
      }
    } catch (error) {
      console.error('Error fetching automation data:', error);
      toast({
        title: 'Error',
        description: 'Failed to fetch task data',
        status: 'error',
        duration: 3000,
        isClosable: true,
      });
    }
  };
  
  // Save the edited script
  const handleSaveScript = async () => {
    if (!automationId) return;
    
    try {
      setIsSaving(true);
      
      // Parse the edited script to validate it's proper JSON
      const scriptObj = JSON.parse(editedScript);
      
      // Save the script
      await invoke('update_automation_script', { 
        automationId, 
        script: scriptObj 
      });
      
      toast({
        title: 'Success',
        description: 'Task updated successfully',
        status: 'success',
        duration: 3000,
        isClosable: true,
      });
      
      // Update the displayed script and exit edit mode
      setAutomationScript(editedScript);
      setIsEditing(false);
    } catch (error) {
      console.error('Error saving script:', error);
      toast({
        title: 'Error',
        description: error instanceof SyntaxError 
          ? 'Invalid JSON format' 
          : 'Failed to save task script',
        status: 'error',
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} size="xl" scrollBehavior="inside">
      <ModalOverlay />
      <ModalContent>
        <ModalHeader>
          {isProcessing ? 'Processing Task' : 'Task Results'}
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody>
          {isProcessing ? (
            <VStack spacing={4} align="stretch">
              <Flex direction="column" align="center" justify="center" py={6}>
                <Spinner size="xl" color="blue.500" mb={4} />
                <Text fontSize="lg" fontWeight="medium" textAlign="center">
                  {status}
                </Text>
                <Progress size="sm" isIndeterminate w="100%" mt={4} colorScheme="blue" />
              </Flex>
            </VStack>
          ) : (
            <VStack spacing={4} align="stretch">
              <Tabs isFitted variant="enclosed" index={tabIndex} onChange={setTabIndex}>
                <TabList mb="1em">
                  <Tab>Human-Readable Steps</Tab>
                  <Tab>Technical Script</Tab>
                </TabList>
                <TabPanels>
                  <TabPanel p={0}>
                    <Box
                      p={3}
                      bg="gray.50"
                      borderRadius="md"
                      fontSize="sm"
                      whiteSpace="pre-wrap"
                      overflowX="auto"
                      minHeight="300px"
                      border="1px"
                      borderColor="gray.200"
                    >
                      {humanReadableScript === "Human-readable summary not available. Using the technical script instead." ? (
                        <Flex direction="column" align="center" justify="center" h="100%" textAlign="center">
                          <Text color="gray.500" mb={4}>
                            Human-readable summary is temporarily disabled.
                          </Text>
                          <Text color="gray.500" mb={4}>
                            Please use the "Technical Script" tab to view the task details.
                          </Text>
                          <Button size="sm" colorScheme="blue" onClick={() => setTabIndex(1)}>
                            Switch to Technical Script
                          </Button>
                        </Flex>
                      ) : (
                        humanReadableScript
                      )}
                    </Box>
                  </TabPanel>
                  <TabPanel p={0}>
                    <Text fontWeight="bold" mb={2}>Generated Task:</Text>
                    
                    {isEditing ? (
                      <Textarea
                        value={editedScript}
                        onChange={(e) => setEditedScript(e.target.value)}
                        minHeight="300px"
                        fontFamily="monospace"
                        fontSize="sm"
                      />
                    ) : (
                      <Box
                        p={3}
                        bg="gray.50"
                        borderRadius="md"
                        fontFamily="monospace"
                        fontSize="sm"
                        whiteSpace="pre-wrap"
                        overflowX="auto"
                        minHeight="300px"
                        border="1px"
                        borderColor="gray.200"
                      >
                        {automationScript}
                      </Box>
                    )}
                  </TabPanel>
                </TabPanels>
              </Tabs>
            </VStack>
          )}
        </ModalBody>

        <ModalFooter>
          {!isProcessing && (
            <>
              {tabIndex === 1 && (
                isEditing ? (
                  <>
                    <Button 
                      variant="outline" 
                      mr={3} 
                      onClick={() => {
                        setEditedScript(automationScript);
                        setIsEditing(false);
                      }}
                    >
                      Cancel
                    </Button>
                    <Button 
                      colorScheme="blue" 
                      onClick={handleSaveScript}
                      isLoading={isSaving}
                    >
                      Save Changes
                    </Button>
                  </>
                ) : (
                  <>
                    <Button variant="outline" mr={3} onClick={() => setIsEditing(true)}>
                      Edit Script
                    </Button>
                    <Button colorScheme="blue" onClick={onClose}>
                      Done
                    </Button>
                  </>
                )
              )}
              
              {tabIndex === 0 && (
                <Button colorScheme="blue" onClick={onClose}>
                  Done
                </Button>
              )}
            </>
          )}
          {isProcessing && (
            <Button variant="outline" onClick={onClose}>
              Close
            </Button>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}; 
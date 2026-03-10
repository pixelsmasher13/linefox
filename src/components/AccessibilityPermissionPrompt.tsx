import React, { useEffect, useState } from 'react';
import {
  Alert,
  AlertIcon,
  AlertTitle,
  AlertDescription,
  Box,
  Button,
  VStack,
  HStack,
  Text,
  Spinner,
  useDisclosure,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  Image,
  OrderedList,
  ListItem,
} from '@chakra-ui/react';
import { invoke } from '@tauri-apps/api/core';
import { CheckCircle, Shield, AlertCircle } from 'lucide-react';

interface AccessibilityPermissionPromptProps {
  onPermissionGranted?: () => void;
  showAsModal?: boolean;
}

export const AccessibilityPermissionPrompt: React.FC<AccessibilityPermissionPromptProps> = ({ 
  onPermissionGranted, 
  showAsModal = false 
}) => {
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [isChecking, setIsChecking] = useState(true);
  const [keyboardMonitoringStarted, setKeyboardMonitoringStarted] = useState(false);
  const { isOpen, onOpen, onClose } = useDisclosure();

  const checkPermissions = async () => {
    try {
      setIsChecking(true);
      const result = await invoke<boolean>('check_accessibility_permissions');
      setHasPermission(result);
      
      if (result) {
        // Start keyboard monitoring when accessibility permissions are granted (only once)
        if (!keyboardMonitoringStarted) {
          try {
            await invoke('start_keyboard_monitoring');
            console.log('Keyboard monitoring started successfully');
            setKeyboardMonitoringStarted(true);
          } catch (error) {
            console.error('Failed to start keyboard monitoring:', error);
          }
        }
        
        if (onPermissionGranted) {
          onPermissionGranted();
        }
      }
      
      if (!result && showAsModal) {
        onOpen();
      }
    } catch (error) {
      console.error('Failed to check accessibility permissions:', error);
      setHasPermission(false);
    } finally {
      setIsChecking(false);
    }
  };

  useEffect(() => {
    checkPermissions();
    
    // Check permissions periodically while the component is mounted
    const interval = setInterval(checkPermissions, 1000);
    
    return () => clearInterval(interval);
  }, []);

  const handleRequestPermissions = async () => {
    try {
      await invoke('prompt_for_accessibility_permissions');
      // The interval will automatically detect when permissions are granted
    } catch (error) {
      console.error('Failed to prompt for accessibility permissions:', error);
    }
  };

  if (isChecking && hasPermission === null) {
    return (
      <Box p={4}>
        <HStack spacing={2}>
          <Spinner size="sm" />
          <Text>Checking accessibility permissions...</Text>
        </HStack>
      </Box>
    );
  }

  if (hasPermission) {
    return null; // Don't show anything if permissions are granted
  }

  const PermissionContent = () => (
    <VStack spacing={4} align="stretch">
      <Alert status="warning" borderRadius="md">
        <AlertIcon>
          <AlertCircle />
        </AlertIcon>
        <Box>
          <AlertTitle>Accessibility & Input Monitoring Permissions Required</AlertTitle>
          <AlertDescription>
            Linefox needs accessibility permissions to observe and control applications on your Mac, and input monitoring to capture keyboard shortcuts.
          </AlertDescription>
        </Box>
      </Alert>

      <Box bg="gray.50" p={4} borderRadius="md">
        <VStack spacing={3} align="start">
          <HStack spacing={2}>
            <Shield size={20} color="#3182CE" />
            <Text fontWeight="semibold">Why do we need this permission?</Text>
          </HStack>
          <Text fontSize="sm" color="gray.600">
            Accessibility permissions allow Linefox to:
          </Text>
          <OrderedList fontSize="sm" color="gray.600" pl={4}>
            <ListItem>Observe application interfaces and user interactions</ListItem>
            <ListItem>Execute tasks to save you time</ListItem>
            <ListItem>Interact with buttons, text fields, and other UI elements</ListItem>
            <ListItem>Capture keyboard shortcuts (Cmd+C, Cmd+V, etc.) for more accurate task execution</ListItem>
          </OrderedList>
        </VStack>
      </Box>

      <VStack spacing={3}>
        <Button
          colorScheme="blue"
          onClick={handleRequestPermissions}
          width="full"
          size="lg"
          leftIcon={<Shield />}
        >
          Grant Accessibility Permission
        </Button>
        
        <Text fontSize="xs" color="gray.500" textAlign="center">
          After clicking, you'll see a system dialog. Enable Linefox in System Preferences → Security & Privacy → Accessibility
        </Text>
      </VStack>

      {!hasPermission && !isChecking && (
        <Box bg="blue.50" p={3} borderRadius="md">
          <HStack spacing={2} align="start">
            <CheckCircle size={16} color="#3182CE" style={{ marginTop: 2 }} />
            <VStack align="start" spacing={1}>
              <Text fontSize="sm" fontWeight="medium">Waiting for permission...</Text>
              <Text fontSize="xs" color="gray.600">
                This message will disappear automatically once you grant the permission.
              </Text>
            </VStack>
          </HStack>
        </Box>
      )}
    </VStack>
  );

  if (showAsModal) {
    return (
      <Modal isOpen={isOpen} onClose={onClose} size="lg" closeOnOverlayClick={false}>
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Setup Required</ModalHeader>
          <ModalBody>
            <PermissionContent />
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" onClick={onClose}>
              I'll do this later
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    );
  }

  return (
    <Box p={6} bg="white" borderRadius="lg" shadow="sm" border="1px solid" borderColor="gray.200">
      <PermissionContent />
    </Box>
  );
};
import React, { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  Box,
  Button,
  Flex,
  Input,
  Text,
  VStack,
  useToast,
  Textarea,
  Heading,
  Divider,
  Alert,
  AlertIcon,
  AlertTitle,
  AlertDescription,
} from '@chakra-ui/react';

interface AppResult {
  success: boolean;
  message: string;
}

export const ChromeAutomation: React.FC = () => {
  const [appName, setAppName] = useState('Google Chrome');
  const [isLoading, setIsLoading] = useState(false);
  const [result, setResult] = useState('');
  const toast = useToast();

  const handleLaunchApp = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<AppResult>('launch_app', { appName });
      
      setResult(JSON.stringify(result, null, 2));
      
      toast({
        title: result.success ? 'Success' : 'Error',
        description: result.message,
        status: result.success ? 'success' : 'error',
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error('Error launching app:', error);
      setResult(JSON.stringify(error, null, 2));
      
      toast({
        title: 'Error',
        description: `Failed to launch app: ${error}`,
        status: 'error',
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleGetAppUIElements = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<AppResult>('get_app_actionable_elements', { appName });
      
      setResult(JSON.stringify(result, null, 2));
      
      toast({
        title: result.success ? 'Success' : 'Error',
        description: result.success ? `Retrieved ${appName} UI elements` : result.message,
        status: result.success ? 'success' : 'error',
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error(`Error getting ${appName} UI elements:`, error);
      setResult(JSON.stringify(error, null, 2));
      
      toast({
        title: 'Error',
        description: `Failed to get ${appName} UI elements: ${error}`,
        status: 'error',
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handlePerformRandomAction = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<AppResult>('perform_random_app_action', { appName });
      
      setResult(JSON.stringify(result, null, 2));
      
      toast({
        title: result.success ? 'Success' : 'Error',
        description: result.message,
        status: result.success ? 'success' : 'error',
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error('Error performing random action:', error);
      setResult(JSON.stringify(error, null, 2));
      
      toast({
        title: 'Error',
        description: `Failed to perform random action: ${error}`,
        status: 'error',
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <Box p={4}>
      <Heading size="md" mb={4}>App Control</Heading>
      <Divider mb={4} />
      
      <Alert status="info" mb={4}>
        <AlertIcon />
        <Box>
          <AlertTitle>Accessibility Permissions Required</AlertTitle>
          <AlertDescription>
            This feature requires macOS accessibility permissions. Please ensure the target app is installed and running.
          </AlertDescription>
        </Box>
      </Alert>
      
      <VStack spacing={4} align="start">
        <Flex width="100%">
          <Input
            placeholder="App Name (e.g. Google Chrome, Safari, Notes)"
            value={appName}
            onChange={(e) => setAppName(e.target.value)}
            mr={2}
          />
          <Button 
            colorScheme="green" 
            isLoading={isLoading} 
            onClick={handleLaunchApp}
          >
            Launch App
          </Button>
        </Flex>
        
        <Flex width="100%" wrap="wrap" gap={2}>
          <Button 
            colorScheme="teal" 
            isLoading={isLoading} 
            onClick={handleGetAppUIElements}
          >
            Get App UI Elements
          </Button>
          <Button
            colorScheme="blue"
            isLoading={isLoading}
            onClick={handlePerformRandomAction}
          >
            Perform Random Action
          </Button>
        </Flex>
        
        <Text fontWeight="bold" mt={4}>Result:</Text>
        <Textarea 
          value={result} 
          height="300px" 
          isReadOnly={true} 
          fontFamily="monospace"
        />
      </VStack>
    </Box>
  );
}; 
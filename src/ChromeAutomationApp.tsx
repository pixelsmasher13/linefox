import React from 'react';
import { ChakraProvider, Box, Container } from '@chakra-ui/react';
import { ChromeAutomation } from './components/ChromeAutomation';

export const AppAutomation: React.FC = () => {
  return (
    <ChakraProvider>
      <Container maxW="container.md" py={5}>
        <Box boxShadow="md" p={5} borderRadius="md" bg="white">
          <ChromeAutomation />
        </Box>
      </Container>
    </ChakraProvider>
  );
};

export default AppAutomation; 
import React from "react";
import { Flex, Box, VStack, HStack, Text as ChakraText } from "@chakra-ui/react";
import { ChatMessage, PhasePlanMetadata } from "../../chatTypes";

export const PhasePlanBubble: React.FC<{ message: ChatMessage }> = ({ message }) => {
  const meta = message.metadata as PhasePlanMetadata;
  if (!meta) return null;

  return (
    <Flex justify="flex-start" mb={3}>
      <Box
        bg="white"
        border="1px solid"
        borderColor="gray.200"
        borderLeft="3px solid"
        borderLeftColor="purple.500"
        px={4}
        py={3.5}
        borderRadius="xl"
        borderTopLeftRadius="sm"
        maxW="92%"
      >
        <VStack align="stretch" spacing={3}>
          <HStack spacing={2} align="center">
            <Flex
              bg="purple.600"
              color="white"
              px={2}
              py={0.5}
              borderRadius="sm"
              fontSize="xs"
              fontWeight="bold"
              letterSpacing="wider"
            >
              {`PHASE ${meta.phaseNumber}`}
            </Flex>
            <ChakraText fontSize="md" fontWeight="semibold" color="gray.900">
              {meta.phaseName}
            </ChakraText>
          </HStack>

          {meta.goal && (
            <Box>
              <ChakraText fontSize="xs" fontWeight="bold" color="gray.500" textTransform="uppercase" letterSpacing="wider" mb={1}>
                Goal
              </ChakraText>
              <ChakraText fontSize="sm" color="gray.800" lineHeight="1.55">
                {meta.goal}
              </ChakraText>
            </Box>
          )}

          {meta.steps.length > 0 && (
            <Box>
              <ChakraText fontSize="xs" fontWeight="bold" color="gray.500" textTransform="uppercase" letterSpacing="wider" mb={1.5}>
                {`Plan · ${meta.steps.length} steps`}
              </ChakraText>
              <VStack align="stretch" spacing={1.5}>
                {meta.steps.map((step, idx) => (
                  <HStack key={idx} align="flex-start" spacing={2.5}>
                    <ChakraText fontSize="sm" color="gray.400" fontFamily="mono" lineHeight="1.55" flexShrink={0} minW="18px" fontWeight="semibold">
                      {idx + 1}.
                    </ChakraText>
                    <ChakraText fontSize="sm" color="gray.800" lineHeight="1.55">
                      {step}
                    </ChakraText>
                  </HStack>
                ))}
              </VStack>
            </Box>
          )}

          {meta.nextPhaseHint && (
            <ChakraText fontSize="sm" color="gray.500" fontStyle="italic">
              Next: {meta.nextPhaseHint}
            </ChakraText>
          )}
        </VStack>
      </Box>
    </Flex>
  );
};

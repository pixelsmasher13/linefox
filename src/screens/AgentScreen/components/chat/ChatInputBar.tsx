import React, { useState, useRef } from "react";
import {
  Box,
  Flex,
  Textarea,
  IconButton,
  Tooltip,
  Text as ChakraText,
} from "@chakra-ui/react";
import { Send, Square, ChevronDown, Zap, ListChecks } from "lucide-react";

type InputMode = "idle" | "planning" | "executing" | "completed";

interface ChatInputBarProps {
  mode: InputMode;
  onSendPrompt: (text: string, agentMode: boolean) => void;
  onSendMessage: (text: string) => void;
  onContinue: (text: string) => void;
  onStop: () => void;
  agentMode: boolean;
  onAgentModeChange: (val: boolean) => void;
  disabled?: boolean;
  /** When true, removes border/bg chrome for use in centered empty state */
  centered?: boolean;
}

export const ChatInputBar: React.FC<ChatInputBarProps> = ({
  mode,
  onSendPrompt,
  onSendMessage,
  onContinue,
  onStop,
  agentMode,
  onAgentModeChange,
  disabled,
  centered,
}) => {
  const [input, setInput] = useState("");
  const [modeMenuOpen, setModeMenuOpen] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const placeholder = (() => {
    switch (mode) {
      case "planning":
        return "Planning...";
      case "executing":
        return "Send a message to the agent...";
      case "completed":
        return "Continue this task... (e.g., 'Now also export to PDF')";
      default:
        return "Tell me what you want done and I'll figure out the rest...";
    }
  })();

  const isDisabled = disabled || mode === "planning";

  const handleSend = () => {
    const text = input.trim();
    if (!text) return;
    if (mode === "executing") {
      onSendMessage(text);
    } else if (mode === "completed") {
      onContinue(text);
    } else {
      onSendPrompt(text, agentMode);
    }
    setInput("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <Box
      px={centered ? 0 : 4}
      py={centered ? 0 : 3}
      borderTop={centered ? "none" : "1px solid"}
      borderColor="gray.200"
      bg={centered ? "transparent" : "white"}
      flexShrink={0}
      width="100%"
    >
      <Flex maxW="700px" mx="auto" direction="column" gap={0}>
        {/* Input container */}
        <Box
          border="1px solid"
          borderColor="gray.200"
          borderRadius={centered ? "20px" : "12px"}
          bg="gray.50"
          overflow="hidden"
          _focusWithin={{
            borderColor: "blue.300",
            boxShadow: "0 0 0 1px var(--chakra-colors-blue-300)",
            bg: "white",
          }}
          transition="all 0.15s"
        >
          <Textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => {
              setInput(e.target.value);
              e.target.style.height = "auto";
              e.target.style.height = `${Math.min(e.target.scrollHeight, centered ? 160 : 140)}px`;
            }}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={1}
            resize="none"
            isDisabled={isDisabled}
            fontSize={centered ? "md" : "sm"}
            border="none"
            borderRadius={0}
            minH={centered ? "64px" : "44px"}
            maxH={centered ? "160px" : "140px"}
            px={4}
            pt={centered ? 4 : 3}
            pb={1}
            overflow="auto"
            bg="transparent"
            _focus={{
              boxShadow: "none",
              border: "none",
            }}
          />

          {/* Bottom bar inside input: send button on right */}
          <Flex
            align="center"
            justify="flex-end"
            px={3}
            pb={2}
            pt={0}
          >
            {mode === "executing" && (
              <Tooltip label="Stop execution">
                <IconButton
                  aria-label="Stop"
                  icon={<Square size={14} />}
                  size="xs"
                  colorScheme="red"
                  variant="outline"
                  borderRadius="full"
                  onClick={onStop}
                  mr={1}
                />
              </Tooltip>
            )}
            <IconButton
              aria-label="Send"
              icon={<Send size={14} />}
              size="xs"
              colorScheme="blue"
              variant={input.trim() ? "solid" : "ghost"}
              borderRadius="full"
              onClick={handleSend}
              isDisabled={!input.trim() || isDisabled}
            />
          </Flex>
        </Box>

        {/* Mode & model selectors — outside the input container */}
        {mode === "idle" && (
          <Flex mt={2} px={1} gap={2}>
            {/* Mode selector */}
            <Box position="relative">
              <Flex
                as="button"
                align="center"
                gap={1.5}
                px={2.5}
                py={1}
                borderRadius="full"
                border="1px solid"
                borderColor={modeMenuOpen ? "blue.300" : "gray.200"}
                bg={modeMenuOpen ? "blue.50" : "transparent"}
                color={agentMode ? "blue.600" : "gray.600"}
                fontSize="xs"
                fontWeight="medium"
                cursor="pointer"
                transition="all 0.15s"
                _hover={{ borderColor: "blue.300", bg: "blue.50" }}
                onClick={() => setModeMenuOpen(!modeMenuOpen)}
                w="fit-content"
              >
                {agentMode ? <Zap size={12} /> : <ListChecks size={12} />}
                <ChakraText fontSize="xs" fontWeight="medium">
                  {agentMode ? "Agent Mode" : "Task Mode"}
                </ChakraText>
                <ChevronDown size={11} />
              </Flex>

              {modeMenuOpen && (
                <>
                  <Box
                    position="fixed"
                    top={0}
                    left={0}
                    right={0}
                    bottom={0}
                    zIndex={10}
                    onClick={() => setModeMenuOpen(false)}
                  />
                  <Box
                    position="absolute"
                    top="calc(100% + 4px)"
                    left={0}
                    bg="white"
                    border="1px solid"
                    borderColor="gray.200"
                    borderRadius="lg"
                    boxShadow="lg"
                    zIndex={20}
                    minW="240px"
                    py={1}
                  >
                    <Flex
                      as="button"
                      w="full"
                      px={3}
                      py={2.5}
                      align="flex-start"
                      gap={2.5}
                      _hover={{ bg: "gray.50" }}
                      bg={!agentMode ? "blue.50" : "transparent"}
                      onClick={() => {
                        onAgentModeChange(false);
                        setModeMenuOpen(false);
                      }}
                      transition="background 0.1s"
                    >
                      <Box mt="2px" flexShrink={0}>
                        <ListChecks size={14} color={!agentMode ? "#3182ce" : "#718096"} />
                      </Box>
                      <Box textAlign="left">
                        <ChakraText fontSize="sm" fontWeight="medium" color={!agentMode ? "blue.600" : "gray.700"}>
                          Task Mode
                        </ChakraText>
                        <ChakraText fontSize="xs" color="gray.500" mt={0.5}>
                          Step-by-step plan, then executes it
                        </ChakraText>
                      </Box>
                    </Flex>

                    <Flex
                      as="button"
                      w="full"
                      px={3}
                      py={2.5}
                      align="flex-start"
                      gap={2.5}
                      _hover={{ bg: "gray.50" }}
                      bg={agentMode ? "blue.50" : "transparent"}
                      onClick={() => {
                        onAgentModeChange(true);
                        setModeMenuOpen(false);
                      }}
                      transition="background 0.1s"
                    >
                      <Box mt="2px" flexShrink={0}>
                        <Zap size={14} color={agentMode ? "#3182ce" : "#718096"} />
                      </Box>
                      <Box textAlign="left">
                        <ChakraText fontSize="sm" fontWeight="medium" color={agentMode ? "blue.600" : "gray.700"}>
                          Agent Mode
                        </ChakraText>
                        <ChakraText fontSize="xs" color="gray.500" mt={0.5}>
                          Autonomous agent that adapts as it goes
                        </ChakraText>
                      </Box>
                    </Flex>
                  </Box>
                </>
              )}
            </Box>

          </Flex>
        )}
      </Flex>
    </Box>
  );
};

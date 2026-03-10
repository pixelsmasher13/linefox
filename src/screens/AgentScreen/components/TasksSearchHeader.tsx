import React, { useState } from 'react';
import { Flex, Input, IconButton } from '@chakra-ui/react';
import { Search, X } from 'lucide-react';

interface TasksSearchHeaderProps {
  onSearchChange: (query: string) => void;
}

export const TasksSearchHeader: React.FC<TasksSearchHeaderProps> = ({ onSearchChange }) => {
  const [searchQuery, setSearchQuery] = useState("");

  const handleSearchChange = (value: string) => {
    setSearchQuery(value);
    onSearchChange(value);
  };

  const handleClear = () => {
    setSearchQuery("");
    onSearchChange("");
  };

  return (
    <Flex align="center" gap={2}>
      <Search size={14} color="#9ca3af" style={{ flexShrink: 0 }} />
      <Input
        size="sm"
        placeholder="Search..."
        value={searchQuery}
        onChange={(e) => handleSearchChange(e.target.value)}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        sx={{
          fontSize: '13px',
          borderRadius: '6px',
          border: '1px solid',
          borderColor: 'gray.200',
          backgroundColor: 'white',
          height: '32px',
          '&:focus': {
            borderColor: 'gray.300',
            boxShadow: 'none',
          }
        }}
      />
      {searchQuery && (
        <IconButton
          aria-label="Clear search"
          icon={<X size={14} />}
          size="xs"
          variant="ghost"
          color="gray.400"
          onClick={handleClear}
          _hover={{ color: 'gray.600' }}
        />
      )}
    </Flex>
  );
};

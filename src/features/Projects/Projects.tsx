import { type FC, useState, useMemo, useRef } from "react";
import styled from "styled-components";
import { 
  Box, 
  Menu, 
  MenuButton, 
  MenuList, 
  MenuItem, 
  IconButton, 
  Flex, 
  Badge,
  Tooltip,
  Divider,
  Input,
  InputGroup,
  InputLeftElement,
  Text as ChakraText,
  Tag,
  AlertDialog,
  AlertDialogBody,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogContent,
  AlertDialogOverlay,
  Button,
  useDisclosure
} from "@chakra-ui/react";
import { 
  Search, 
  File, 
  Trash2, 
  Edit, 
  X, 
  MoreHorizontal, 
  FilePlus, 
  FolderPlus 
} from 'lucide-react';
import { Text } from "@heelix-app/design";
import { useProject } from "../../state";
import { ProjectModal } from "@/components";
import { type Project } from "../../data/project";

//
// -- Styled Components --
const Container = styled(Box)`
  display: flex;
  flex: 1;
  flex-direction: column;
  padding: var(--space-l);
  gap: var(--space-l);
  width: 100%;
  max-width: 420px; /* Increased width to occupy more space */
  margin: 0 auto;
  overflow: hidden;
`;

const StyledMenuButton = styled(MenuButton)`
  background-color: white;
  border: 1px solid var(--chakra-colors-gray-200);
  border-radius: var(--chakra-radii-md);
  padding: 8px 12px;
  height: 40px;
  display: flex;
  align-items: center;
  width: 100%;
  transition: all 0.2s;
  
  &:hover {
    background-color: var(--chakra-colors-gray-50);
    border-color: var(--chakra-colors-gray-300);
  }
  
  &:focus {
    box-shadow: 0 0 0 2px var(--chakra-colors-blue-100);
    border-color: var(--chakra-colors-blue-500);
  }
`;

const ScrollableMenuList = styled(MenuList)`
  max-height: 300px;
  overflow-y: auto;
  
  /* Custom scrollbar styling */
  &::-webkit-scrollbar {
    width: 8px;
  }
  
  &::-webkit-scrollbar-track {
    background: var(--chakra-colors-gray-100);
    border-radius: 4px;
  }
  
  &::-webkit-scrollbar-thumb {
    background: var(--chakra-colors-gray-300);
    border-radius: 4px;
  }
  
  &::-webkit-scrollbar-thumb:hover {
    background: var(--chakra-colors-gray-400);
  }
`;

const DocumentsContainer = styled(Box)`
  max-height: 500px;
  overflow-y: auto;
  border-radius: var(--chakra-radii-md);
  
  /* Custom scrollbar styling */
  &::-webkit-scrollbar {
    width: 8px;
  }
  
  &::-webkit-scrollbar-track {
    background: transparent;
    border-radius: 4px;
  }
  
  &::-webkit-scrollbar-thumb {
    background: var(--chakra-colors-gray-200);
    border-radius: 4px;
  }
  
  &::-webkit-scrollbar-thumb:hover {
    background: var(--chakra-colors-gray-300);
  }
`;

const ProjectHeader = styled(Box)`
  background-color: var(--chakra-colors-gray-50);
  padding: 8px 12px;
  font-size: 12px;
  font-weight: 500;
  color: var(--chakra-colors-gray-600);
  border-bottom: 1px solid var(--chakra-colors-gray-200);
`;

const DocumentName = styled(ChakraText)`
  font-size: 14px;
  line-height: 1.4;
  font-weight: 400;
  color: var(--chakra-colors-gray-800);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  text-overflow: ellipsis;
  max-height: 40px; /* 2 lines * line height */
  max-width: calc(100% - 60px); /* Added more space for the three dots menu */
  padding-right: 4px; /* Extra padding to ensure separation */
`;

const ProjectTag = styled(Tag)`
  position: absolute;
  bottom: 8px;
  right: 8px;
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 12px;
  background-color: var(--chakra-colors-gray-100);
  color: var(--chakra-colors-gray-600);
  z-index: 1;
`;

const UnassignedTag = styled(Tag)`
  position: absolute;
  bottom: 8px;
  right: 8px;
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 12px;
  background-color: var(--chakra-colors-gray-100);
  color: var(--chakra-colors-gray-500);
  font-style: italic;
  z-index: 1;
`;

const SearchContainer = styled(Box)`
  margin-bottom: 10px;
`;

const truncateDocumentName = (name: string, maxLength: number = 30) => {
  if (name.length <= maxLength) return name;
  return `${name.substring(0, maxLength)}...`;
};

const UNASSIGNED_PROJECT_NAME = "Unassigned";

// DeleteProjectButton component for project deletion
const DeleteProjectButton: FC<{
  project: Project;
  onDelete: (project: Project) => void;
}> = ({ project, onDelete }) => {
  const { isOpen, onOpen, onClose } = useDisclosure();
  const cancelRef = useRef(null);
  
  return (
    <>
      <Tooltip label="Delete this project">
        <IconButton
          aria-label="Delete project"
          icon={<Trash2 size={16} />}
          size="sm"
          variant="ghost"
          onClick={onOpen}
        />
      </Tooltip>
      
      <AlertDialog
        isOpen={isOpen}
        leastDestructiveRef={cancelRef}
        onClose={onClose}
      >
        <AlertDialogOverlay>
          <AlertDialogContent>
            <AlertDialogHeader fontSize="lg" fontWeight="bold">
              Delete Project
            </AlertDialogHeader>
            
            <AlertDialogBody>
              Are you sure you want to delete "{project.name}"? 
              This action cannot be undone.
            </AlertDialogBody>
            
            <AlertDialogFooter>
              <Button ref={cancelRef} onClick={onClose}>
                Cancel
              </Button>
              <Button 
                colorScheme="red" 
                onClick={() => {
                  onDelete(project);
                  onClose();
                }} 
                ml={3}
              >
                Delete
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
    </>
  );
};

//
// -- Main Export --
export const Projects: FC<{
  selectedActivityId: number | null;
  onSelectActivity: (activityId: number | null) => void;
}> = ({ selectedActivityId, onSelectActivity }) => {
  const { 
    state, 
    selectProject, 
    addProject, 
    deleteProject, 
    updateProject,
    updateActivityName,
    addBlankActivity,
    addUnassignedActivity,
    deleteActivity
  } = useProject();
  
  const [modalOpen, setModalOpen] = useState(false);
  const [selectedProjectId, setSelectedProjectId] = useState<null | number>(null);

  const currentProject = useMemo(() => 
    state.projects.find(p => p.id === state.selectedProject),
    [state.projects, state.selectedProject]
  );

  // Filter out the unassigned project for the dropdown
  const visibleProjects = useMemo(() => 
    state.projects.filter(p => p.name !== UNASSIGNED_PROJECT_NAME),
    [state.projects]
  );

  const handleProjectSelect = (project: Project) => {
    selectProject(project.id);
  };

  const handleUnselectProject = () => {
    selectProject(undefined);
  };

  const handleNewProject = () => {
    setSelectedProjectId(null);
    setModalOpen(true);
  };

  const handleEditProject = (project: Project) => {
    setSelectedProjectId(project.id);
    setModalOpen(true);
  };

  const handleDeleteProject = (project: Project) => {
    deleteProject(project.id);
    if (state.selectedProject === project.id) {
      selectProject(undefined);
    }
  };

  const handleClose = () => {
    setModalOpen(false);
    setSelectedProjectId(null);
  };

  const handleActivitySelect = (activityId: number) => {
    onSelectActivity(activityId);
  };

  return (
    <Container>
      <ProjectSelector
        projects={visibleProjects}
        allProjects={state.projects}
        selectedProject={currentProject}
        onSelectProject={handleProjectSelect}
        onUnselectProject={handleUnselectProject}
        onNewProject={handleNewProject}
        onEditProject={handleEditProject}
        onDeleteProject={handleDeleteProject}
        selectedActivityId={selectedActivityId}
        onSelectActivity={handleActivitySelect}
        onUpdateActivityName={updateActivityName}
        onAddBlankActivity={addBlankActivity}
        onAddUnassignedActivity={addUnassignedActivity}
        onDeleteActivity={deleteActivity}
      />
      
      <ProjectModal
        isOpen={modalOpen}
        projectId={selectedProjectId}
        onClose={handleClose}
        onUpdate={updateProject}
        onSave={addProject}
      />
    </Container>
  );
};

//
// -- ProjectSelector Component --
type ActivityDocument = {
  id: number;
  activity_id: number | null;
  name: string;
  projectId: number;
  projectName: string;
};

const ProjectSelector: FC<{
  projects: Project[];
  allProjects: Project[];
  selectedProject: Project | undefined;
  onSelectProject: (project: Project) => void;
  onUnselectProject: () => void;
  onNewProject: () => void;
  onEditProject: (project: Project) => void;
  onDeleteProject: (project: Project) => void;
  selectedActivityId: number | null;
  onSelectActivity: (activityId: number) => void;
  onUpdateActivityName: (activityId: number, name: string) => void;
  onAddBlankActivity: () => Promise<number | undefined>;
  onAddUnassignedActivity: () => Promise<number | undefined>;
  onDeleteActivity: (activityId: number) => void;
}> = ({
  projects,
  allProjects,
  selectedProject,
  onSelectProject,
  onUnselectProject,
  onNewProject,
  onEditProject,
  onDeleteProject,
  selectedActivityId,
  onSelectActivity,
  onUpdateActivityName,
  onAddBlankActivity,
  onAddUnassignedActivity,
  onDeleteActivity
}) => {
  const [editingActivityId, setEditingActivityId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState("");
  const [searchTerm, setSearchTerm] = useState("");
  const [documentSearchTerm, setDocumentSearchTerm] = useState("");

  // Filter projects by name
  const filteredProjects = useMemo(() => {
    if (!searchTerm.trim()) return projects;
    return projects.filter((p) =>
      p.name.toLowerCase().includes(searchTerm.toLowerCase())
    );
  }, [projects, searchTerm]);

  // Get all activities across all projects or from the selected project only
  const allDocuments = useMemo(() => {
    if (selectedProject) {
      // Only return this project's activities
      return selectedProject.activities.map((_, index) => ({
        id: selectedProject.activities[index],
        activity_id: selectedProject.activity_ids[index],
        name: selectedProject.activity_names[index] 
          || `Document ${selectedProject.activities[index]}`,
        projectId: selectedProject.id,
        projectName: selectedProject.name
      }));
    }
    // Otherwise, gather from all projects
    return allProjects.flatMap(project => 
      project.activities.map((_, index) => ({
        id: project.activities[index],
        activity_id: project.activity_ids[index],
        name: project.activity_names[index] 
          || `Document ${project.activities[index]}`,
        projectId: project.id,
        projectName: project.name
      }))
    );
  }, [selectedProject, allProjects]);

  // Filter documents by name/project name
  const filteredDocuments = useMemo(() => {
    if (!documentSearchTerm.trim()) return allDocuments;
    return allDocuments.filter(doc => 
      doc.name.toLowerCase().includes(documentSearchTerm.toLowerCase()) ||
      doc.projectName.toLowerCase().includes(documentSearchTerm.toLowerCase())
    );
  }, [allDocuments, documentSearchTerm]);

  // Sort documents by descending ID for recency
  const sortedDocuments = useMemo(() => {
    return [...filteredDocuments].sort((a, b) => b.id - a.id);
  }, [filteredDocuments]);

  // Start renaming a document
  const handleStartEdit = (activity: { id: number; name: string }) => {
    setEditingActivityId(activity.id);
    setEditingName(activity.name);
  };

  // Save document name change
  const handleSaveEdit = () => {
    if (editingActivityId && editingName.trim()) {
      onUpdateActivityName(editingActivityId, editingName.trim());
      setEditingActivityId(null);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSaveEdit();
    } else if (e.key === 'Escape') {
      setEditingActivityId(null);
    }
  };

  // Create a new document in selected or unassigned project
  const handleAddNewDocument = async () => {
    let newActivityId;
    
    if (selectedProject) {
      newActivityId = await onAddBlankActivity();
    } else {
      newActivityId = await onAddUnassignedActivity();
    }
    
    if (newActivityId) {
      handleStartEdit({ id: newActivityId, name: "New Document" });
    }
  };

  // Select a document without forcing a project switch
  const handleDocumentSelect = (document: ActivityDocument) => {
    onSelectActivity(document.id);
  };

  // Delete a document
  const handleDeleteDocument = (e: React.MouseEvent, document: ActivityDocument) => {
    e.stopPropagation();
    onDeleteActivity(document.id);
  };

  return (
    <Flex direction="column" w="full" gap={4} overflow="hidden">
      <Flex gap={2} w="full" align="center">
        <Menu>
          <Flex position="relative" w="full">
            <StyledMenuButton w="full">
              <Text type="m" bold>
                {selectedProject ? selectedProject.name : 'Select a Project'}
              </Text>
            </StyledMenuButton>

            {selectedProject && (
              <IconButton
                position="absolute"
                right="2"
                top="50%"
                transform="translateY(-50%)"
                aria-label="Unselect project"
                icon={<X size={14} />}
                size="xs"
                variant="ghost"
                zIndex="1"
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  onUnselectProject();
                }}
                _hover={{ bg: 'gray.100' }}
              />
            )}
          </Flex>

          <ScrollableMenuList minW="240px" w="240px" py={0}>
            {/* Sticky search box */}
            <Box 
              p={2} 
              h="56px" 
              display="flex" 
              alignItems="center" 
              position="sticky" 
              top="0" 
              bg="white" 
              zIndex="1"
            >
              <InputGroup size="sm">
                <InputLeftElement pointerEvents="none">
                  <Search size={14} color="var(--chakra-colors-gray-400)" />
                </InputLeftElement>
                <Input
                  placeholder="Search Projects..."
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  autoComplete="off"
                  autoCorrect="off"
                  spellCheck="false"
                />
              </InputGroup>
            </Box>
            <Divider my={0} />

            {/* "Create New Project" at the top */}
            <MenuItem 
              icon={<FolderPlus size={16} />}
              onClick={onNewProject}
              p={3}
              h="40px"
            >
              <Text type="m">Create New Project</Text>
            </MenuItem>
            <Divider my={2} />

            <Box>
              {filteredProjects.map((project) => (
                <MenuItem 
                  key={project.id}
                  onClick={() => onSelectProject(project)}
                  p={3}
                  h="40px"
                >
                  <Flex justify="space-between" align="center" w="full">
                    <Text type="m">{project.name}</Text>
                    <Badge colorScheme="blue" ml={2}>
                      {project.activities.length} docs
                    </Badge>
                  </Flex>
                </MenuItem>
              ))}
            </Box>
          </ScrollableMenuList>
        </Menu>
        {/* Removed the separate "Create New Project" plus icon */}
      </Flex>

      {/* Document list section */}
      <Flex direction="column" w="full">
        <Flex justify="space-between" align="center" mb={3}>
          <Text type="m" bold>
            {selectedProject ? `${selectedProject.name} Documents` : "All Documents"}
          </Text>
          
          <Flex gap={2}>
            {/* Button for creating a new document (FilePlus icon + tooltip) */}
            <Tooltip label="Create a new document">
              <IconButton
                aria-label="Add new document"
                icon={<FilePlus size={16} />}
                size="sm"
                variant="ghost"
                onClick={handleAddNewDocument}
              />
            </Tooltip>
            
            {/* Only show delete button when a project is selected */}
            {selectedProject && (
              <DeleteProjectButton 
                project={selectedProject} 
                onDelete={onDeleteProject} 
              />
            )}
          </Flex>
        </Flex>
        
        <SearchContainer mb={3}>
          <InputGroup size="md">
            <InputLeftElement pointerEvents="none">
              <Search size={16} color="var(--chakra-colors-gray-400)" />
            </InputLeftElement>
            <Input
              placeholder="Search documents..."
              value={documentSearchTerm}
              onChange={(e) => setDocumentSearchTerm(e.target.value)}
              autoComplete="off"
              autoCorrect="off"
              spellCheck="false"
              borderRadius="full"
              _focus={{
                boxShadow: "0 0 0 1px var(--chakra-colors-blue-400)",
                borderColor: "blue.400"
              }}
            />
          </InputGroup>
        </SearchContainer>
        
        <DocumentsContainer>
          <Box>
            {sortedDocuments.length > 0 ? (
              sortedDocuments.map((document) => (
                <Flex
                  key={document.id}
                  p={3}
                  mb={1}
                  borderRadius="md"
                  align="center"
                  justify="space-between"
                  _hover={{ bg: 'gray.50' }}
                  transition="all 0.2s"
                  bg={selectedActivityId === document.id ? 'blue.50' : 'white'}
                  onClick={() => editingActivityId !== document.id && handleDocumentSelect(document)}
                  cursor="pointer"
                  position="relative"
                  minHeight="55px"
                  role="group"
                >
                  <Flex align="center" gap={3} flex={1}>
                    <Box color="gray.500">
                      <File size={16} />
                    </Box>
                    <Box flex={1}>
                      {editingActivityId === document.id ? (
                        <Input
                          value={editingName}
                          onChange={(e: React.ChangeEvent<HTMLInputElement>) => setEditingName(e.target.value)}
                          onBlur={handleSaveEdit}
                          onKeyDown={handleKeyDown}
                          onClick={(e: React.MouseEvent) => e.stopPropagation()}
                          autoFocus
                          size="sm"
                          variant="unstyled"
                        />
                      ) : (
                        <Box>
                          <DocumentName>
                            {truncateDocumentName(document.name)}
                          </DocumentName>
                          
                          {/* Show project tag only if no project filter is applied */}
                          {!selectedProject && (
                            document.projectName === UNASSIGNED_PROJECT_NAME ? (
                              <></>
                            ) : (
                              <ProjectTag size="sm" variant="subtle">
                                {document.projectName}
                              </ProjectTag>
                            )
                          )}
                        </Box>
                      )}
                    </Box>
                  </Flex>
                  
                  {/* Three dots menu in the top-right corner */}
                  {!editingActivityId && (
                    <Menu placement="bottom-end" isLazy>
                      <MenuButton
                        as={IconButton}
                        aria-label="Document options"
                        icon={<MoreHorizontal size={14} />}
                        size="xs"
                        variant="ghost"
                        opacity="0"
                        _groupHover={{ opacity: 1 }}
                        onClick={(e: React.MouseEvent) => e.stopPropagation()}
                        position="absolute"
                        top="2"
                        right="2"
                      />
                      <MenuList minW="150px">
                        <MenuItem
                          icon={<Edit size={14} />}
                          onClick={(e: React.MouseEvent) => {
                            e.stopPropagation();
                            handleStartEdit(document);
                          }}
                        >
                          Rename
                        </MenuItem>
                        <MenuItem
                          icon={<Trash2 size={14} />}
                          onClick={(e: React.MouseEvent) => handleDeleteDocument(e, document)}
                          color="red.500"
                        >
                          Delete
                        </MenuItem>
                      </MenuList>
                    </Menu>
                  )}
                </Flex>
              ))
            ) : (
              <Flex 
                justify="center" 
                align="center" 
                p={8}
                color="gray.500"
                flexDirection="column"
                gap={2}
              >
                <File size={24} />
                <Text type="m">
                  {documentSearchTerm 
                    ? "No matching documents found" 
                    : selectedProject 
                      ? "No documents added yet" 
                      : "No documents added yet"}
                </Text>
              </Flex>
            )}
          </Box>
        </DocumentsContainer>
      </Flex>
    </Flex>
  );
};
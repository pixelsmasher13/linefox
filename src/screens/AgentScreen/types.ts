export interface Automation {
  id: number;
  name: string;
  objective: string;
  nl_description?: string;
  created_at: string;
  updated_at: string;
}

export interface AutomationStep {
  id: string;
  type: 'click' | 'type' | 'wait' | 'navigate' | 'keypress' | 'custom';
  target?: string;
  value?: string;
  description: string;
}

export interface AutomationScript {
  id: number;
  name: string;
  description: string;
  goal?: string;
  objective: string;
  nl_description?: string;
  additional_instructions?: string;
  steps: AutomationStep[];
  created_at: string;
  updated_at: string;
} 
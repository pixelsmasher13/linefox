import {
  Newspaper,
  Plane,
  Code,
  Target,
  TrendingUp,
  Megaphone,
  ShoppingCart,
  Scale,
  LucideIcon,
} from 'lucide-react';

export interface CategoryConfig {
  icon: LucideIcon;
  label: string;
  prompts: string[];
}

// Category prompts for new users - focused on high-frequency tasks on complex sites
export const CATEGORY_PROMPTS: Record<string, CategoryConfig> = {
  daily: {
    icon: Newspaper,
    label: 'Daily Tasks',
    prompts: [
      "Review today's tech articles from BBC, NYTimes, and FT",
      "Research robotic vacuums on Amazon",
      "Follow top 10 AI researchers on my Twitter",
    ]
  },
  travel: {
    icon: Plane,
    label: 'Travel',
    prompts: [
      "Find direct flights from SF to NYC on Feb 15-16",
      "Compare 2BR Airbnbs in Park City for MLK weekend",
      "Find SUV rentals in Miami under $80/day on Kayak",
    ]
  },
  coding: {
    icon: Code,
    label: 'Coding',
    prompts: [
      "Inspect Vercel logs for warnings or errors in the last hour",
      "Review top 10 trending repos on GitHub and summarize",
      "Close all resolved GitHub issues from last week",
    ]
  },
  sales: {
    icon: Target,
    label: 'Sales',
    prompts: [
      "Find 20 marketing directors in Bay Area on LinkedIn",
      "Add my saved LinkedIn leads to HubSpot",
      "Go through today's emails and add new contacts to Salesforce",
    ]
  },
  finance: {
    icon: TrendingUp,
    label: 'Finance',
    prompts: [
      "Review and summarize all expert calls for NVDA on AlphaSense",
      "Download transcripts for PLTR earnings calls on FactSet",
      "Find Apple's 2024 10-K filing on SEC Edgar",
    ]
  },
  marketing: {
    icon: Megaphone,
    label: 'Marketing',
    prompts: [
      "Improve headlines on 5 worst performing ads on my Facebook Ads",
      "Reply to unanswered DMs on IG based on these guidelines",
      "Change the promotional headline in our Shopify theme",
    ]
  },
  ecommerce: {
    icon: ShoppingCart,
    label: 'E-commerce',
    prompts: [
      "Check MRR and retention data on Stripe",
      "Add attached product SKUs to my Shopify store",
      "Review my competitor websites for latest prices",
    ]
  },
  legal: {
    icon: Scale,
    label: 'Legal',
    prompts: [
      "Download NY court filings for case 818973/2025E",
      "Find Judge Chen's rulings on motion to dismiss in SDNY",
      "Research OpenAI patent applications on USPTO",
    ]
  },
};

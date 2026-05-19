import {
  Newspaper,
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

// Category prompts — desktop-native workflows across browser, apps, and CLI
export const CATEGORY_PROMPTS: Record<string, CategoryConfig> = {
  daily: {
    icon: Newspaper,
    label: 'Daily Tasks',
    prompts: [
      "Go through my unread emails, draft replies for anything urgent, and add action items to my Notes app",
      "Open Slack and summarize what I missed overnight across my top 5 channels",
      "Find the best-rated espresso machines on Amazon under $500 and put a comparison into an Excel spreadsheet",
    ]
  },
  coding: {
    icon: Code,
    label: 'Coding',
    prompts: [
      "Build a React app with Claude CLI, open it in Chrome to test, then push to GitHub and deploy on Vercel",
      "Check my Vercel dashboard for failed deployments, use Claude CLI to fix the errors, then redeploy and verify in the browser",
      "Go to my GitHub repo issues, pick the top 3 bugs, fix them with Claude CLI, then open a PR on GitHub with a summary",
    ]
  },
  sales: {
    icon: Target,
    label: 'Sales',
    prompts: [
      "Find 20 VP of Engineering profiles at Series B startups on LinkedIn and add them as contacts in HubSpot",
      "Go through today's Gmail inbox, identify new inbound leads, and create deals for each in Salesforce",
      "Find 10 companies that just raised Series A on Crunchbase, look up their CTOs on LinkedIn, and draft intro emails referencing their funding round",
    ]
  },
  finance: {
    icon: TrendingUp,
    label: 'Finance',
    prompts: [
      "Review and summarize all expert calls for NVDA on AlphaSense",
      "Download PLTR and SNOW earnings transcripts from FactSet and build a side-by-side comparison in Excel",
      "Research Tesla's latest financials online and put together an income statement forecast in Excel",
    ]
  },
  marketing: {
    icon: Megaphone,
    label: 'Marketing',
    prompts: [
      "Write a blog post about our product launch, publish it on WordPress, then share it on Twitter and LinkedIn",
      "Check our Facebook Ads manager, find the 5 worst performing ads, and rewrite their headlines",
      "Go to Canva, create a social media graphic for our sale, download it, and post it on Instagram",
    ]
  },
  ecommerce: {
    icon: ShoppingCart,
    label: 'E-commerce',
    prompts: [
      "Check Stripe for this month's MRR and churn numbers, then update my revenue tracker in Excel",
      "Visit our top 5 competitor websites, collect their current pricing, and put it all in a comparison spreadsheet",
      "Add these 10 new product SKUs to my Shopify store with descriptions, images, and pricing",
    ]
  },
  legal: {
    icon: Scale,
    label: 'Legal',
    prompts: [
      "Search PACER for recent filings in my case, download the PDFs, and organize them in my Documents folder",
      "Research recent patent applications from OpenAI on USPTO and summarize findings in a Word doc",
      "Look up Judge Chen's latest rulings on motion to dismiss in SDNY and compile key excerpts into a brief",
    ]
  },
};

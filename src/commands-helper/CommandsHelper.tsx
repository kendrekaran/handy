import React, { useState } from "react";
import "./CommandsHelper.css";

interface CommandCategory {
  title: string;
  commands: { voice: string; description: string }[];
}

const COMMAND_CATEGORIES: CommandCategory[] = [
  {
    title: "Open Websites",
    commands: [
      { voice: "Open YouTube", description: "Opens YouTube" },
      { voice: "Open Google", description: "Opens Google" },
      { voice: "Open Gmail", description: "Opens Gmail" },
      { voice: "Open GitHub", description: "Opens GitHub" },
      { voice: "Open ChatGPT", description: "Opens ChatGPT" },
      { voice: "Open Claude", description: "Opens Claude" },
      { voice: "Open Twitter", description: "Opens Twitter/X" },
      { voice: "Open LinkedIn", description: "Opens LinkedIn" },
      { voice: "Open Reddit", description: "Opens Reddit" },
      { voice: "Open WhatsApp", description: "Opens WhatsApp" },
      { voice: "Open Notion", description: "Opens Notion" },
      { voice: "Open Spotify", description: "Opens Spotify" },
      { voice: "Open Stack Overflow", description: "Opens Stack Overflow" },
    ],
  },
  {
    title: "Text Editing",
    commands: [
      { voice: "Select All", description: "Select all text" },
      { voice: "Copy", description: "Copy selection" },
      { voice: "Cut", description: "Cut selection" },
      { voice: "Paste the text", description: "Paste clipboard" },
      { voice: "Undo", description: "Undo last action" },
      { voice: "Redo", description: "Redo last action" },
      { voice: "Save", description: "Save file" },
      { voice: "Find", description: "Open find/search" },
    ],
  },
  {
    title: "Clear Text",
    commands: [
      { voice: "Clear one word", description: "Delete 1 word" },
      { voice: "Clear 3 words", description: "Delete 3 words" },
      { voice: "Clear line", description: "Clear current line" },
      { voice: "Clear all", description: "Clear all text" },
    ],
  },
  {
    title: "Tab Management",
    commands: [
      { voice: "New tab", description: "Open new tab" },
      { voice: "Close tab", description: "Close current tab" },
      { voice: "Reopen tab", description: "Restore closed tab" },
      { voice: "Next tab", description: "Switch to next tab" },
      { voice: "Previous tab", description: "Switch to prev tab" },
    ],
  },
  {
    title: "Window Management",
    commands: [
      { voice: "New window", description: "Open new window" },
      { voice: "Close window", description: "Close window" },
      { voice: "Go left", description: "Snap window left" },
      { voice: "Go right", description: "Snap window right" },
      { voice: "Maximize", description: "Maximize window" },
      { voice: "Minimize", description: "Minimize window" },
    ],
  },
  {
    title: "Navigation",
    commands: [
      { voice: "Go back", description: "Browser back" },
      { voice: "Go forward", description: "Browser forward" },
      { voice: "Scroll up", description: "Scroll page up" },
      { voice: "Scroll down", description: "Scroll page down" },
      { voice: "Scroll to top", description: "Go to page top" },
      { voice: "Scroll to bottom", description: "Go to page bottom" },
      { voice: "Refresh", description: "Reload page" },
      { voice: "Address bar", description: "Focus URL bar" },
    ],
  },
  {
    title: "Keyboard Shortcuts",
    commands: [
      { voice: "Command C", description: "Cmd+C (copy)" },
      { voice: "Command V", description: "Cmd+V (paste)" },
      { voice: "Command A", description: "Cmd+A (select all)" },
      { voice: "Command S", description: "Cmd+S (save)" },
      { voice: "Command Z", description: "Cmd+Z (undo)" },
      { voice: "Command Shift T", description: "Cmd+Shift+T" },
      { voice: "Control Z", description: "Ctrl+Z" },
      { voice: "Option Backspace", description: "Alt+Backspace" },
      { voice: "Command Enter", description: "Cmd+Enter" },
      { voice: "Command <key>", description: "Any Cmd shortcut" },
      { voice: "Control <key>", description: "Any Ctrl shortcut" },
      { voice: "Shift <key>", description: "Any Shift shortcut" },
    ],
  },
  {
    title: "Other",
    commands: [
      { voice: "Take screenshot", description: "Capture screenshot" },
      { voice: "Zoom in", description: "Zoom in" },
      { voice: "Zoom out", description: "Zoom out" },
      { voice: "Reset zoom", description: "Reset zoom level" },
      { voice: "Mute", description: "Mute system audio" },
      { voice: "Unmute", description: "Unmute system audio" },
    ],
  },
];

const CommandsHelper: React.FC = () => {
  const [expandedCategory, setExpandedCategory] = useState<string | null>(null);

  const toggleCategory = (title: string) => {
    setExpandedCategory(expandedCategory === title ? null : title);
  };

  return (
    <div className="commands-helper">
      <div className="commands-header">
        <span className="commands-title">Voice Commands</span>
      </div>
      <div className="commands-body">
        {COMMAND_CATEGORIES.map((category) => (
          <div key={category.title} className="command-category">
            <div
              className="category-header"
              onClick={() => toggleCategory(category.title)}
            >
              <span className="category-title">{category.title}</span>
              <span
                className={`category-chevron ${expandedCategory === category.title ? "expanded" : ""}`}
              >
                <svg
                  width="10"
                  height="6"
                  viewBox="0 0 10 6"
                  fill="none"
                  xmlns="http://www.w3.org/2000/svg"
                >
                  <path
                    d="M1 1L5 5L9 1"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              </span>
            </div>
            {expandedCategory === category.title && (
              <div className="category-commands">
                {category.commands.map((cmd) => (
                  <div key={cmd.voice} className="command-row">
                    <span className="command-voice">{cmd.voice}</span>
                    <span className="command-desc">{cmd.description}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

export default CommandsHelper;

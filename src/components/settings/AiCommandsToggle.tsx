import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AiCommandsToggleProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AiCommandsToggle: React.FC<AiCommandsToggleProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("ai_commands_enabled") || false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("ai_commands_enabled", enabled)}
        isUpdating={isUpdating("ai_commands_enabled")}
        label={t("settings.debug.aiCommandsToggle.label")}
        description={t("settings.debug.aiCommandsToggle.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);

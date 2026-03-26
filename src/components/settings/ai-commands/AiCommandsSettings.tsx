import React, { useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui";
import { Input } from "../../ui/Input";
import { useSettings } from "../../../hooks/useSettings";

export const AiCommandsSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const apiKey = getSetting("ai_commands_api_key") || "";
  const [draftApiKey, setDraftApiKey] = useState(apiKey);

  // Sync draft when setting changes externally
  React.useEffect(() => {
    setDraftApiKey(apiKey);
  }, [apiKey]);

  const handleApiKeyBlur = useCallback(() => {
    if (draftApiKey !== apiKey) {
      updateSetting("ai_commands_api_key", draftApiKey);
    }
  }, [draftApiKey, apiKey, updateSetting]);

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.aiCommands.api.title")}>
        <SettingContainer
          title={t("settings.aiCommands.api.apiKey.title")}
          description={t("settings.aiCommands.api.apiKey.description")}
          descriptionMode="tooltip"
          layout="horizontal"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            <Input
              type="password"
              value={draftApiKey}
              onChange={(e) => setDraftApiKey(e.target.value)}
              onBlur={handleApiKeyBlur}
              placeholder={t("settings.aiCommands.api.apiKey.placeholder")}
              disabled={isUpdating("ai_commands_api_key")}
              variant="compact"
              className="min-w-[320px]"
            />
          </div>
        </SettingContainer>

        <SettingContainer
          title={t("settings.aiCommands.api.model.title")}
          description={t("settings.aiCommands.api.model.description")}
          descriptionMode="tooltip"
          layout="horizontal"
          grouped={true}
        >
          <div className="flex items-center gap-2">
            <span className="text-sm text-mid-gray">MiniMax-M2.7</span>
          </div>
        </SettingContainer>
      </SettingsGroup>

      <SettingsGroup title={t("settings.aiCommands.info.title")}>
        <div className="p-4 text-sm text-mid-gray space-y-2">
          <p>{t("settings.aiCommands.info.description")}</p>
          <p className="text-xs opacity-70">
            {t("settings.aiCommands.info.howItWorks")}
          </p>
        </div>
      </SettingsGroup>
    </div>
  );
};

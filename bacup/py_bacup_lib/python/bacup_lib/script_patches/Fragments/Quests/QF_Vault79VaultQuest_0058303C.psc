Function Fragment_Stage_0100_Item_00()
    If Vault79VaultQuest_Ventilation && !Vault79VaultQuest_Ventilation.IsPlaying()
        Vault79VaultQuest_Ventilation.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If Vault79VaultQuest_SentryBotScene && !Vault79VaultQuest_SentryBotScene.IsPlaying()
        Vault79VaultQuest_SentryBotScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If Vault79VaultQuest_GoldAnalysisScene && !Vault79VaultQuest_GoldAnalysisScene.IsPlaying()
        Vault79VaultQuest_GoldAnalysisScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If MTNZ05_Messenger_DeliveryScene != None && !MTNZ05_Messenger_DeliveryScene.IsPlaying()
        MTNZ05_Messenger_DeliveryScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

Function Fragment_Stage_0600_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

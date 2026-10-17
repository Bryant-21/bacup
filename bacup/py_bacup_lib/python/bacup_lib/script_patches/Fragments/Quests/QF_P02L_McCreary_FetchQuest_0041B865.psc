Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    SuppliesCount = 0
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(21, True)
    SuppliesCount += 1
    If SuppliesCount >= 3 && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetObjectiveCompleted(22, True)
    SuppliesCount += 1
    If SuppliesCount >= 3 && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    SetObjectiveCompleted(23, True)
    SuppliesCount += 1
    If SuppliesCount >= 3 && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(21, True)
    SetObjectiveDisplayed(22, True)
    SetObjectiveDisplayed(23, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

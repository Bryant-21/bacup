Function Fragment_Stage_0150_Item_00()
    ObjectReference playerRef = Alias_Player.GetReference()
    If playerRef != None && CageKey != None
        playerRef.AddItem(CageKey, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    SetObjectiveDisplayed(40, True)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(40, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(30, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(20, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(50, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

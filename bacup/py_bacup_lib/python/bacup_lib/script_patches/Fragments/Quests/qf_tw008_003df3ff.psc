Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(500, True)
EndFunction

Function Fragment_Stage_0610_Item_00()
    SetObjectiveCompleted(600, True)
EndFunction

Function Fragment_Stage_0710_Item_00()
    SetObjectiveCompleted(700, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(150, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(700, True)
EndFunction

Function Fragment_Stage_0990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_0999_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

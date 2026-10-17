Function Fragment_Stage_1000_Item_00()
    RETriggerScript triggerScript = Alias_TRIGGER.GetReference() as RETriggerScript
    If triggerScript != None
        triggerScript.ReArmTrigger()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteAllObjectives()
    ; B21:QuestRewards / CurrencyQuestRewards pay on OnStageSet; let it dispatch before shutdown.
    Utility.Wait(1.0)
    Stop()
EndFunction

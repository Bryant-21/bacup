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
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

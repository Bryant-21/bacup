Function Fragment_Stage_1000_Item_00()
    RETriggerScript triggerScript = Alias_TRIGGER.GetReference() as RETriggerScript
    If triggerScript != None
        triggerScript.ReArmTrigger()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_0500_Item_01()
    FailAllObjectives()
    Stop()
EndFunction

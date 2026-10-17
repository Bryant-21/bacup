Function Fragment_Stage_1000_Item_00()
    RETriggerScript triggerScript = Alias_TRIGGER.GetReference() as RETriggerScript
    If triggerScript != None
        triggerScript.ReArmTrigger()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteAllObjectives()
    ; The snack scene's completion sets 400 (End Encounter), so the quest must outlive it.
    If RE_ScenePS02_SnackTimeScene != None && !RE_ScenePS02_SnackTimeScene.IsPlaying()
        RE_ScenePS02_SnackTimeScene.Start()
    ElseIf RE_ScenePS02_SnackTimeScene == None && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    Stop()
EndFunction

Quests:E01C_Tales:Dark:QuestScript Function GetEventScript()
    Quest owner = Self as Quest
    Return owner as Quests:E01C_Tales:Dark:QuestScript
EndFunction

Function Fragment_Stage_0010_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.DisableCampfireMechanic()
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    If !IsStageDone(900)
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.EnsureTaleSelected()
    EndIf
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0110_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.SelectTale(110)
    EndIf
EndFunction

Function Fragment_Stage_0120_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.SelectTale(120)
    EndIf
EndFunction

Function Fragment_Stage_0130_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.SelectTale(130)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(20)
    If Scene_01 && !Scene_01.IsPlaying()
        Scene_01.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(25)
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.BeginShadowsActivity()
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(25)
    If Scene_02 && !Scene_02.IsPlaying()
        Scene_02.Start()
    EndIf
EndFunction

Function Fragment_Stage_0380_Item_00()
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30)
    If Scene_03 && !Scene_03.IsPlaying()
        Scene_03.Start()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(40)
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.BeginKindlingGathering()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(40)
    If Scene_04 && !Scene_04.IsPlaying()
        Scene_04.Start()
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(50)
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.PlaceEvidence()
    EndIf
EndFunction

Function RefreshEvidenceTargets()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.RefreshEvidenceTargets()
    EndIf
EndFunction

Function Fragment_Stage_0710_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0720_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0730_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0740_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0750_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0760_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0770_Item_00()
    RefreshEvidenceTargets()
EndFunction

Function Fragment_Stage_0780_Item_00()
    RefreshEvidenceTargets()
EndFunction

; Scene_HoloIntro plays the found holotape and sets 900 on completion. Scene_BringHolo belongs to the
; cut "bring the holotape to Penny" flow and sets no stage, so starting it here stalled the event.
Function Fragment_Stage_0850_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
    If Scene_HoloIntro && !Scene_HoloIntro.IsPlaying()
        Scene_HoloIntro.Start()
    ElseIf !Scene_HoloIntro
        SetStage(900)
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(60)
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.StartBossFight()
    EndIf
    ; Only the Ronnie / Bug Swarm ending (stage 120) runs the numbered insect waves
    ; that objective 85 and these three messages describe.
    If GetStageDone(120) && E01C_Tales_Dark_Wave1
        E01C_Tales_Dark_Wave1.Show()
    EndIf
EndFunction

Function Fragment_Stage_0921_Item_00()
    If E01C_Tales_Dark_Wave2
        E01C_Tales_Dark_Wave2.Show()
    EndIf
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.AdvanceInsectWave(921)
    EndIf
EndFunction

Function Fragment_Stage_0922_Item_00()
    If E01C_Tales_Dark_WaveFinal
        E01C_Tales_Dark_WaveFinal.Show()
    EndIf
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.AdvanceInsectWave(922)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.FinishEvent(False)
    Else
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.FinishEvent(True)
    Else
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_9991_Item_00()
    SetObjectiveFailed(10)
    Quests:E01C_Tales:Dark:QuestScript eventScript = GetEventScript()
    If eventScript != None
        eventScript.FinishEvent(True)
    Else
        Stop()
    EndIf
EndFunction

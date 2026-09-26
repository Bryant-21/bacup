MTNS04QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as MTNS04QuestScript
EndFunction

Function SetAliasEnabled(ReferenceAlias akAlias, Bool abEnabled)
    If akAlias == None || akAlias.GetReference() == None
        Return
    EndIf
    If abEnabled
        akAlias.GetReference().Enable(False)
    Else
        akAlias.GetReference().Disable(False)
    EndIf
EndFunction

Function ResetEventObjectives()
    Int[] objectives = New Int[5]
    objectives[0] = 100
    objectives[1] = 200
    objectives[2] = 250
    objectives[3] = 260
    objectives[4] = 300
    Int index = 0
    While index < objectives.Length
        SetObjectiveDisplayed(objectives[index], False)
        SetObjectiveCompleted(objectives[index], False)
        SetObjectiveFailed(objectives[index], False)
        index += 1
    EndWhile
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function HideOptionalObjectives()
    SetObjectiveDisplayed(250, False)
    SetObjectiveDisplayed(260, False)
EndFunction

Function EndEventActivity()
    HideOptionalObjectives()
    MTNS04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndActivity()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    SetAliasEnabled(Alias_JukeboxSoundMarker, False)
    MTNS04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetActivity()
    EndIf
    SetObjectiveDisplayed(100, True, True)
EndFunction

; DefaultAliasOnActivate on the Jukebox alias sets this stage.
Function Fragment_Stage_0110_Item_00()
    If IsStageDone(9990) || IsStageDone(450)
        Return
    EndIf
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
    ; FO76 runs the 720 s quest timer from here: the 600 s attract window plus the 120 s Nightstalker window.
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None && !questTimer.IsQuestTimerRunning()
        questTimer.StartQuestTimer()
    EndIf
    If !IsStageDone(200)
        SetStage(200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    MTNS04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartAggroActivity()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(200, True)
    HideOptionalObjectives()
    SetObjectiveDisplayed(300, True, True)
    MTNS04QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartWendigoFight()
    EndIf
EndFunction

; The boss wave's StageToSetAtEnd sets this stage once the Nightstalker is dead.
Function Fragment_Stage_0400_Item_00()
    If IsStageDone(450)
        Return
    EndIf
    SetObjectiveCompleted(300, True)
    EndEventActivity()
EndFunction

Function Fragment_Stage_0450_Item_00()
    If IsStageDone(400)
        Return
    EndIf
    FailOpenObjective(100)
    FailOpenObjective(200)
    FailOpenObjective(300)
    EndEventActivity()
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetAliasEnabled(Alias_LightEnableMarker, False)
    SetAliasEnabled(Alias_JukeboxSoundMarker, False)
EndFunction

; B21:ObjectiveTimers sets this stage when nobody turned on the jukebox during the prep window.
Function Fragment_Stage_9990_Item_00()
    If IsStageDone(110)
        Return
    EndIf
    FailOpenObjective(100)
    EndEventActivity()
EndFunction

; Objective timers on 200/300 and B21:QuestTimer set this stage when the attract or kill window runs out.
Function Fragment_Stage_9991_Item_00()
    If IsStageDone(400) || IsStageDone(450)
        Return
    EndIf
    SetStage(450)
EndFunction

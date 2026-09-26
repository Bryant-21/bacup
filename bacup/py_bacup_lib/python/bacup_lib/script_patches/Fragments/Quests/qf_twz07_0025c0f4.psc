Bool Function IsEventOver()
    Return IsStageDone(0) || IsStageDone(999) || IsStageDone(1000)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(100)
    ResetEventObjective(200)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function PlayMayorScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

DefaultQuestEncounterWaveScript Function ParadeWaves()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartParade()
    DefaultQuestEncounterWaveScript waveScript = ParadeWaves()
    If waveScript != None
        ; The single parade wave carries no IDString, so it can only be started by index.
        waveScript.StartEncounterWave(0)
    EndIf
EndFunction

Function StopParade()
    DefaultQuestEncounterWaveScript waveScript = ParadeWaves()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function ShowKillObjective()
    If IsEventOver()
        Return
    EndIf
    CompleteOpenObjective(100)
    SetObjectiveDisplayed(200, True, True)
    PlayMayorScene(TWZ07MayorWorried)
EndFunction

Function ShutdownEvent(Bool abFailed)
    StopParade()
    If abFailed
        FailOpenObjective(100)
        FailOpenObjective(200)
    EndIf
    StartTimer(5.0, 25074)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 25074
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0000_Item_00()
    ShutdownEvent(True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(100, True, True)
    PlayMayorScene(TWZ07MayorWarn)
    StartParade()
    Quest owner = Self as Quest
    TWZ07_Script eventScript = owner as TWZ07_Script
    If eventScript != None
        eventScript.BeginLateJoinCountdown(TooLateToJoinSeconds)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    ShowKillObjective()
EndFunction

Function Fragment_Stage_0300_Item_00()
    ShowKillObjective()
EndFunction

Function Fragment_Stage_0999_Item_00()
    ShutdownEvent(True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteOpenObjective(100)
    CompleteOpenObjective(200)
    If TWZ07MayorMonsterDead && !TWZ07MayorMonsterDead.IsPlaying()
        TWZ07MayorMonsterDead.Start()
    EndIf
    ShutdownEvent(False)
EndFunction

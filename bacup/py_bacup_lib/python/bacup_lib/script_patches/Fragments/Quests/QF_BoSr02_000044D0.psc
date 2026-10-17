; Placement N starts encounter wave N-1 (EnemySpawner, EnemySpawner02, EnemySpawner03). The wave authors no StageToSetAtEnd, so
; clearing it is what completes that placement's scan: the QUST has no other setter for stages 650, 750 and 900.
Function StartPlacementWave(Scene akPlacement, Int aiWaveIndex)
    If akPlacement != None && !akPlacement.IsPlaying()
        akPlacement.Start()
    EndIf
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waves != None
        RegisterForCustomEvent(waves, "WaveComplete")
        waves.StartEncounterWave(aiWaveIndex)
    EndIf
EndFunction

Event DefaultQuestEncounterWaveScript.WaveComplete(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    If akArgs == None || akArgs.Length < 1
        Return
    EndIf
    Int waveIndex = akArgs[0] as Int
    If waveIndex == 0 && IsStageDone(600) && !IsStageDone(650)
        SetStage(650)
    ElseIf waveIndex == 1 && IsStageDone(700) && !IsStageDone(750)
        SetStage(750)
    ElseIf waveIndex == 2 && IsStageDone(800) && !IsStageDone(900)
        SetStage(900)
    EndIf
EndEvent

Function Fragment_Stage_0001_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

; The briefing holotape and the schematic can be found in either order, so each completes its own objective.
Function Fragment_Stage_0200_Item_00()
    SetObjectiveDisplayed(300, True)
    SetObjectiveCompleted(300, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500, True)
    StartPlacementWave(pBoSr02_Placement01, 0)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveDisplayed(550, True)
EndFunction

Function Fragment_Stage_0675_Item_00()
    SetObjectiveCompleted(550, True)
    SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(600, True)
    StartPlacementWave(pBoSr02_Placement02, 1)
EndFunction

Function Fragment_Stage_0750_Item_00()
    SetObjectiveDisplayed(650, True)
EndFunction

Function Fragment_Stage_0775_Item_00()
    SetObjectiveCompleted(650, True)
    SetObjectiveDisplayed(700, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(700, True)
    StartPlacementWave(pBoSr02_Placement03, 2)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveDisplayed(750, True)
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveCompleted(750, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteQuest()
EndFunction

mtns06questscript Function EventScript()
    Quest owner = Self as Quest
    Return owner as mtns06questscript
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function AbortEvent()
    FailOpenObjective(25)
    FailOpenObjective(50)
    FailOpenObjective(100)
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.FinishEvent(False)
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetActivityState()
    EndIf
    SetObjectiveDisplayed(50, True, True)
    SetObjectiveDisplayed(25, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(100, True, True)
    mtns06questscript eventScript = EventScript()
    Actor playerRef = Game.GetPlayer()
    If eventScript != None && eventScript.PlayersIdentified != None && playerRef != None && eventScript.PlayersIdentified.Find(playerRef) < 0
        eventScript.PlayersIdentified.AddRef(playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If MTNS06_Uranium_PA_ActivityStart && !MTNS06_Uranium_PA_ActivityStart.IsPlaying()
        MTNS06_Uranium_PA_ActivityStart.Start()
    EndIf
    SetObjectiveCompleted(50, True)
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(25, True)
    SetObjectiveDisplayed(200, True, True)
    SetObjectiveDisplayed(210, True, True)
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartActivity()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    If MTNS06_Uranium_PA_EnemiesSpawning && !MTNS06_Uranium_PA_EnemiesSpawning.IsPlaying()
        MTNS06_Uranium_PA_EnemiesSpawning.Start()
    EndIf
    MTNS06_WaveScript waves = (Self as Quest) as MTNS06_WaveScript
    If waves != None && !IsStageDone(300) && !IsStageDone(325)
        waves.StartEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
    If MTNS06_Uranium_PA_BossSpawn && !MTNS06_Uranium_PA_BossSpawn.IsPlaying()
        MTNS06_Uranium_PA_BossSpawn.Start()
    EndIf
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartBossWave(1)
    EndIf
EndFunction

Function Fragment_Stage_0202_Item_00()
    If MTNS06_Uranium_PA_BossSpawn && !MTNS06_Uranium_PA_BossSpawn.IsPlaying()
        MTNS06_Uranium_PA_BossSpawn.Start()
    EndIf
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartBossWave(2)
    EndIf
EndFunction

Function Fragment_Stage_0203_Item_00()
    If MTNS06_Uranium_PA_BossSpawn && !MTNS06_Uranium_PA_BossSpawn.IsPlaying()
        MTNS06_Uranium_PA_BossSpawn.Start()
    EndIf
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.StartBossWave(3)
    EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.BossKilled()
    EndIf
EndFunction

Function Fragment_Stage_0212_Item_00()
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.BossKilled()
    EndIf
EndFunction

Function Fragment_Stage_0213_Item_00()
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.BossKilled()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If MTNS06_Uranium_PA_ActivityEnd && !MTNS06_Uranium_PA_ActivityEnd.IsPlaying()
        MTNS06_Uranium_PA_ActivityEnd.Start()
    EndIf
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.FinishEvent(True)
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    If MTNS06_Uranium_PA_ActivityEnd && !MTNS06_Uranium_PA_ActivityEnd.IsPlaying()
        MTNS06_Uranium_PA_ActivityEnd.Start()
    EndIf
    mtns06questscript eventScript = EventScript()
    If eventScript != None
        eventScript.FinishEvent(False)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    AbortEvent()
EndFunction

Function Fragment_Stage_0400_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    AbortEvent()
EndFunction

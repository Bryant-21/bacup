DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

FSS01_TrapQuest Function TrapScript()
    Quest owner = Self as Quest
    Return owner as FSS01_TrapQuest
EndFunction

Function PlayLureAnimation(String asAnimation)
    If asAnimation == "" || Lure == None
        Return
    EndIf
    ObjectReference lureRef = Lure.GetReference()
    If lureRef != None
        lureRef.PlayAnimation(asAnimation)
    EndIf
EndFunction

Function ResetEventObjectives()
    SetObjectiveDisplayed(100, False)
    SetObjectiveCompleted(100, False)
    SetObjectiveFailed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveCompleted(200, False)
    SetObjectiveFailed(200, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveCompleted(300, False)
    SetObjectiveFailed(300, False)
    SetObjectiveDisplayed(400, False)
    SetObjectiveCompleted(400, False)
    SetObjectiveFailed(400, False)
EndFunction

Function GroundScorchbeast()
    Actor scorchbeast = None
    If Alias_Scorchbeast != None
        scorchbeast = Alias_Scorchbeast.GetActorReference()
    EndIf
    If scorchbeast == None
        Return
    EndIf

    Quest owner = Self as Quest
    ReferenceAlias landingAlias = owner.GetAlias(23) as ReferenceAlias
    ObjectReference landingRef = None
    If landingAlias != None
        landingRef = landingAlias.GetReference()
    EndIf
    If landingRef == None && Lure != None
        landingRef = Lure.GetReference()
    EndIf
    If landingRef != None && scorchbeast.GetDistance(landingRef) > 2048.0
        scorchbeast.MoveTo(landingRef)
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        scorchbeast.StartCombat(playerRef)
    EndIf
EndFunction

Function EndEventRun(Bool abFailed)
    If abFailed
        FailAllObjectives()
    Else
        CompleteAllObjectives()
    EndIf
    PlayLureAnimation(IdleStart)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf

    FSS01_TrapQuest trapScript = TrapScript()
    If trapScript != None
        trapScript.ArmEventShutdown(20.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    ResetEventObjectives()
    PlayLureAnimation(IdleStart)
    SetObjectiveDisplayed(100, True, True)
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(200)
    PlayLureAnimation(LureAnim)
    SetObjectiveDisplayed(300, True, True)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StartEncounterWaveByID("Scorched")
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    PlayLureAnimation(TrapAnim)
    SetObjectiveDisplayed(400, True, True)
    GroundScorchbeast()
EndFunction

Function Fragment_Stage_0900_Item_00()
    EndEventRun(True)
EndFunction

Function Fragment_Stage_0950_Item_00()
    EndEventRun(True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    EndEventRun(False)
EndFunction

Function Fragment_Stage_2000_Item_00()
    PlayLureAnimation(IdleStart)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(True)
    EndIf
    If Alias_Scorched != None
        Alias_Scorched.RemoveAll()
    EndIf

    FSS01_TrapQuest trapScript = TrapScript()
    If trapScript != None
        trapScript.ApplyCooldown()
    EndIf
EndFunction

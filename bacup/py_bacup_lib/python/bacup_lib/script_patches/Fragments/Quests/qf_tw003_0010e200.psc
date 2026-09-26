Bool Function IsEventOver()
    Return IsStageDone(999) || IsStageDone(1000)
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

DefaultQuestEncounterWaveScript Function EventWaves()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartEventWave(String asWaveID)
    DefaultQuestEncounterWaveScript waveScript = EventWaves()
    If waveScript != None
        waveScript.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWaves()
    DefaultQuestEncounterWaveScript waveScript = EventWaves()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
EndFunction

Bool Function BossWaveStillSpawning()
    DefaultQuestEncounterWaveScript waveScript = EventWaves()
    If waveScript == None
        Return False
    EndIf
    Int bossWave = waveScript.FindEncounterWaveIndex("Boss - Mad Dog Malone")
    Return bossWave >= 0 && waveScript.IsEncounterWaveSpawning(bossWave)
EndFunction

Actor Function FindSpawnedMadDog()
    Actor madDog
    Int index = 0
    While madDog == None && Alias_TempMadDogMalone != None && index < Alias_TempMadDogMalone.GetCount()
        madDog = Alias_TempMadDogMalone.GetAt(index) as Actor
        index += 1
    EndWhile
    Return madDog
EndFunction

Actor Function PlaceMadDog()
    ObjectReference marker
    If Alias_MadDogMarker != None
        marker = Alias_MadDogMarker.GetReference()
    EndIf
    If marker == None || LvlSupermutantMinigunBoss == None
        Return None
    EndIf
    Return marker.PlaceActorAtMe(LvlSupermutantMinigunBoss)
EndFunction

Function PrepareMadDog(Actor akMadDog)
    If akMadDog == None
        Return
    EndIf
    ; The alias keeps him Essential; FO4's DefaultAliasOnEnterBleedout dropped
    ; SetNoBleedoutRecoveryOnInit, so subdual has to be made final here.
    akMadDog.SetNoBleedoutRecovery(True)
EndFunction

Function TrackMadDog()
    CancelTimer(10021)
    If Alias_MadDogMalone == None || !IsRunning() || IsEventOver() || !IsStageDone(100)
        Return
    EndIf

    Actor madDog = Alias_MadDogMalone.GetActorReference()
    If madDog == None
        madDog = FindSpawnedMadDog()
        If madDog == None && BossWaveStillSpawning()
            StartTimer(2.0, 10021)
            Return
        EndIf
        If madDog == None
            madDog = PlaceMadDog()
        EndIf
        If madDog == None
            Return
        EndIf
        Alias_MadDogMalone.ForceRefTo(madDog)
        PrepareMadDog(madDog)
    EndIf

    If IsStageDone(200)
        Return
    EndIf
    If madDog.IsDead() || madDog.IsBleedingOut()
        SetStage(200)
        Return
    EndIf
    StartTimer(5.0, 10021)
EndFunction

Function TakeMadDogIntoCustody()
    If Alias_MadDogMalone == None
        Return
    EndIf
    Actor madDog = Alias_MadDogMalone.GetActorReference()
    If madDog == None
        Return
    EndIf
    ; Captives are friends with everyone, so the marshal can collect him without a firefight.
    If CaptiveFaction != None && !madDog.IsInFaction(CaptiveFaction)
        madDog.AddToFaction(CaptiveFaction)
    EndIf
EndFunction

Function DispatchMarshal()
    Actor marshal
    If Alias_Marshal != None
        marshal = Alias_Marshal.GetActorReference()
    EndIf
    If marshal != None
        marshal.Enable()
        marshal.EvaluatePackage()
    EndIf
    Int seconds = WaitForMarshal
    If seconds <= 0
        seconds = 60
    EndIf
    StartTimer(seconds as Float, 10022)
EndFunction

Function ShutdownEvent(Bool abFailed)
    CancelTimer(10021)
    CancelTimer(10022)
    StopEventWaves()
    If abFailed
        FailOpenObjective(100)
        FailOpenObjective(200)
    EndIf
    StartTimer(5.0, 10029)
EndFunction

Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    ; Papyrus timers do not always survive a reload; the tracker may still owe stage 200.
    If IsRunning() && !IsEventOver() && IsStageDone(100) && !IsStageDone(200)
        TrackMadDog()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 10021
        TrackMadDog()
    ElseIf aiTimerID == 10022
        If IsRunning() && !IsEventOver() && IsStageDone(200)
            SetStage(1000)
        EndIf
    ElseIf aiTimerID == 10029
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0050_Item_00()
    If Alias_MadDogMalone != None
        PrepareMadDog(Alias_MadDogMalone.GetActorReference())
    EndIf
    TrackMadDog()
EndFunction

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(100, True, True)
    StartEventWave("Boss - Mad Dog Malone")
    TrackMadDog()
EndFunction

Function Fragment_Stage_0150_Item_00()
    If IsEventOver()
        Return
    EndIf
    StartEventWave("Mad Dog Super Mutants")
EndFunction

Function Fragment_Stage_0200_Item_00()
    CancelTimer(10021)
    CompleteOpenObjective(100)
    TakeMadDogIntoCustody()
    StopEventWaves()
    If !IsEventOver()
        SetObjectiveDisplayed(200, True, True)
        DispatchMarshal()
    EndIf
EndFunction

Function Fragment_Stage_0999_Item_00()
    ShutdownEvent(True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteOpenObjective(200)
    ShutdownEvent(False)
EndFunction

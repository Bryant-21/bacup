; Debug skips set only checkpoint stages; combat and scene-driven stages stay unset.
Function Fragment_Stage_0001_Item_00()
    GHL00_SkipToCheckpoint(3)
    ObjectReference marker = Alias_Marker_GhoulCamp_IntroPlayer.GetReference()
    If marker != None
        Game.GetPlayer().MoveTo(marker)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    GHL00_SkipToCheckpoint(5)
EndFunction

Function Fragment_Stage_0003_Item_00()
    GHL00_SkipToCheckpoint(9)
EndFunction

Function GHL00_SkipToCheckpoint(Int count)
    Int[] checkpoints = new Int[9]
    checkpoints[0] = 100
    checkpoints[1] = 200
    checkpoints[2] = 300
    checkpoints[3] = 410
    checkpoints[4] = 500
    checkpoints[5] = 600
    checkpoints[6] = 700
    checkpoints[7] = 900
    checkpoints[8] = 1000
    Int index = 0
    While index < count
        If !IsStageDone(checkpoints[index])
            SetStage(checkpoints[index])
        EndIf
        index += 1
    EndWhile
EndFunction

; The Whitespring Leamon and responder are enabled by source state; the refuge
; instance setup has nothing left to place in a persistent FO4 world.
Function Fragment_Stage_0010_Item_00()
EndFunction

; Source InstSwap scripts show the Whitespring Leamon while this AV is below 1,
; and the camp Leamon is initially disabled with packages from stage 300 on.
Function Fragment_Stage_0020_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_LeamonPrice_Whitespring_AwayValue != None
        playerRef.SetValue(GHL00_LeamonPrice_Whitespring_AwayValue, 1.0)
    EndIf
    Actor leamon = Alias_Actor_Leamon_GhoulCamp.GetActorReference()
    If leamon != None
        leamon.Enable()
        leamon.EvaluatePackage()
    EndIf
EndFunction

; Camp Asher has no package between stages 500 and 1000 while the Emmett Asher
; and first feral group are initially disabled and must exist to set stage 550.
Function Fragment_Stage_0030_Item_00()
    ; A first visit after the conclusion (debug skip) must not pull Asher from camp.
    If IsStageDone(1000)
        Return
    EndIf
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_KevanAsherton_AwayValue != None
        playerRef.SetValue(GHL00_KevanAsherton_AwayValue, 1.0)
    EndIf
    Actor asher = Alias_Actor_Asher_Emmett.GetActorReference()
    If asher != None
        asher.Enable()
        asher.EvaluatePackage()
    EndIf
    Alias_Actors_FeralGhouls.EnableAll()
    Alias_Actors_FeralGhouls.EvaluateAll()
EndFunction

; Camp Asher's lab package resumes after stage 1000.
Function Fragment_Stage_0040_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_KevanAsherton_AwayValue != None
        playerRef.SetValue(GHL00_KevanAsherton_AwayValue, 0.0)
    EndIf
    Actor asher = Alias_Actor_Asher_GhoulCamp.GetActorReference()
    If asher != None
        asher.EvaluatePackage()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    Alias_Player.ForceRefIfEmpty(playerRef)
    SetObjectiveDisplayed(10)
    If GHL00_Quest_Radio_QuestStartKeyword != None
        GHL00_Quest_Radio_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
EndFunction

; The Whitespring trigger sets 250 with no prerequisite, so the radio can arrive late.
Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    If !IsStageDone(250)
        SetObjectiveDisplayed(20)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(10)
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(25)
    If Scene_GHL00_000_Whitespring_OnApproach != None
        Scene_GHL00_000_Whitespring_OnApproach.Start()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(25)
    SetObjectiveDisplayed(30)
    If GHL00_Quest_Radio != None
        GHL00_Quest_Radio.SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(35)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(35)
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0550_Item_00()
    If Scene_GHL00_085_EmmettMnt_OnApproach != None
        Scene_GHL00_085_EmmettMnt_OnApproach.Start()
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_Quest_EmmettMountainSideTunnel_GateKey != None
        If playerRef.GetItemCount(GHL00_Quest_EmmettMountainSideTunnel_GateKey) == 0
            playerRef.AddItem(GHL00_Quest_EmmettMountainSideTunnel_GateKey, 1)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(50)
    If Scene_GHL00_10_EmmettMnt_UnlockedGate != None
        Scene_GHL00_10_EmmettMnt_UnlockedGate.Start()
    EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveDisplayed(60)
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waves != None
        Int waveIndex = waves.FindEncounterWaveIndex("Feral Ghouls")
        If waveIndex >= 0 && !waves.WaveUsesCatalog(waveIndex) && Alias_Actors_FeralGhouls_Wave2.GetCount() == 0
            GHL00_SpawnLocalFeralWave(waves.EncounterWaves[waveIndex].SpawnArea)
        EndIf
        waves.StartEncounterWaveByID("Feral Ghouls")
    EndIf
EndFunction

; The source wave selects by ActorTypeFeralGhoul, which no WAVE record carries,
; so FO76's EMS pool is unavailable. Stage 820 comes only from this collection's
; death script, so an empty wave would deadlock the quest. Mirror the quest's own
; first feral group (base and size) at the source spawn center instead.
Function GHL00_SpawnLocalFeralWave(ReferenceAlias spawnArea)
    ObjectReference center = None
    If spawnArea != None
        center = spawnArea.GetReference()
    EndIf
    ActorBase feralBase = None
    Int index = 0
    While feralBase == None && index < Alias_Actors_FeralGhouls.GetCount()
        Actor feral = Alias_Actors_FeralGhouls.GetAt(index) as Actor
        If feral != None
            feralBase = feral.GetActorBase()
        EndIf
        index += 1
    EndWhile
    If center == None || feralBase == None
        SetStage(820)
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    index = 0
    While index < Alias_Actors_FeralGhouls.GetCount()
        Actor spawned = center.PlaceActorAtMe(feralBase)
        Alias_Actors_FeralGhouls_Wave2.AddRef(spawned)
        spawned.StartCombat(playerRef)
        index += 1
    EndWhile
EndFunction

Function Fragment_Stage_0820_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70)
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If waves != None
        waves.StopEncounterWaveByID("Feral Ghouls")
    EndIf
    Actor madeline = Alias_Actor_Madeline.GetActorReference()
    If madeline != None
        madeline.Enable()
        If Alias_Marker_Emmett_MadelineStart.GetReference() != None
            madeline.MoveTo(Alias_Marker_Emmett_MadelineStart.GetReference())
        EndIf
    EndIf
    If GHL00_11_EmmettMnt_MadelineGreeting != None
        GHL00_11_EmmettMnt_MadelineGreeting.Start()
    EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(70)
    SetObjectiveDisplayed(72)
EndFunction

Function Fragment_Stage_0950_Item_00()
    Actor asher = Alias_Actor_Asher_Emmett.GetActorReference()
    ObjectReference marker = Alias_Marker_Emmett_AsherConfrontation.GetReference()
    If asher != None && marker != None
        asher.MoveTo(marker)
    EndIf
EndFunction

Function Fragment_Stage_0955_Item_00()
    SetObjectiveCompleted(72)
    SetObjectiveDisplayed(74)
EndFunction

Function Fragment_Stage_0960_Item_00()
    Actor madeline = Alias_Actor_Madeline.GetActorReference()
    If madeline != None && CaptiveFaction != None
        madeline.RemoveFromFaction(CaptiveFaction)
        madeline.EvaluatePackage()
    EndIf
    ; No record sets 995; Asher's follow-up topics and the exit-location 1000 setter require it.
    SetStage(995)
EndFunction

Function Fragment_Stage_0990_Item_00()
    Actor madeline = Alias_Actor_Madeline.GetActorReference()
    If madeline != None && !madeline.IsDead()
        madeline.Kill(Alias_Actor_Asher_Emmett.GetActorReference())
    EndIf
    SetStage(995)
EndFunction

Function Fragment_Stage_0995_Item_00()
    SetObjectiveCompleted(74)
    SetObjectiveDisplayed(75)
EndFunction

; The exit package is gated on 1000; starting it earlier blocks the talk-to-Asher step.
Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(75)
    SetObjectiveDisplayed(80)
    If Scene_GHL00_Quest_EmmettMnt_AsherLeaves != None && !Scene_GHL00_Quest_EmmettMnt_AsherLeaves.IsPlaying()
        Scene_GHL00_Quest_EmmettMnt_AsherLeaves.Start()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    SetObjectiveCompleted(80)
    SetObjectiveDisplayed(90)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_1305_Item_00()
    SetObjectiveCompleted(100)
EndFunction

; Every reader tests this AV on the player: 1 = Leamon ghoulified, 2 = he stays
; human and leaves (InstSwap ghoul ref, TransformPlayer exit package and scene).
Function Fragment_Stage_1380_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_Quest_LeamonGhoulified_AV != None
        playerRef.SetValue(GHL00_Quest_LeamonGhoulified_AV, 1.0)
    EndIf
    SetStage(1400)
EndFunction

Function Fragment_Stage_1390_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_Quest_LeamonGhoulified_AV != None
        playerRef.SetValue(GHL00_Quest_LeamonGhoulified_AV, 2.0)
    EndIf
    SetStage(1400)
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(90)
    SetObjectiveDisplayed(100, False)
EndFunction

Function Fragment_Stage_1500_Item_00()
    SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    CompleteQuest()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && GHL00_Quest_TransformPlayer_StartKeyword != None
        GHL00_Quest_TransformPlayer_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If GHL00_02_GhoulCamp_AsherConfrontation != None && !GHL00_02_GhoulCamp_AsherConfrontation.IsPlaying()
        GHL00_02_GhoulCamp_AsherConfrontation.Start()
    EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
    If Scene_GHL00_14_GhoulCamp_BrewGhoulJuice != None && !Scene_GHL00_14_GhoulCamp_BrewGhoulJuice.IsPlaying()
        Scene_GHL00_14_GhoulCamp_BrewGhoulJuice.Start()
    EndIf
EndFunction

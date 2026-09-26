Event OnQuestInit()
    currentNumberOfPlacedBombs = 0.0
    currentBombAlias = None
    B21TalkNPC = None
    B21TalkScene = None
    B21TalkStage = -1
    B21PendingScene = None
    B21PendingSceneStage = -1
    B21PendingSceneSeconds = 0.0
EndEvent

Event OnQuestShutdown()
    CleanupEvent()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 68301
        TickExplosivePhase()
    ElseIf aiTimerID == 68302
        TickTalkFallback()
    ElseIf aiTimerID == 68303
        TickPendingScene()
    EndIf
EndEvent

defaultquestencounterwavescript Function EventWaves()
    Quest owner = Self as Quest
    Return owner as defaultquestencounterwavescript
EndFunction

Function StartEventWave(String asWaveID)
    defaultquestencounterwavescript waves = EventWaves()
    If waves != None
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWave(String asWaveID)
    defaultquestencounterwavescript waves = EventWaves()
    If waves != None
        waves.StopEncounterWaveByID(asWaveID, False)
    EndIf
EndFunction

Function StopAllEventWaves(Bool abRemoveActors)
    defaultquestencounterwavescript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(abRemoveActors)
    EndIf
EndFunction

Function SetQuestVariable(String asName, Float afValue)
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable(asName, afValue)
    EndIf
EndFunction

; Luca's greeting normally starts the kickoff scene whose "[Start Event]" reply sets
; the talk stage. When activating Luca plays no scene, start it (or advance) here.
Function BeginTalkFallback(ReferenceAlias akSpeaker, Scene akKickoffScene, Int aiTalkStage)
    EndTalkFallback()
    B21TalkNPC = akSpeaker
    B21TalkScene = akKickoffScene
    B21TalkStage = aiTalkStage
    If akSpeaker != None && akSpeaker.GetReference() != None
        RegisterForRemoteEvent(akSpeaker.GetReference(), "OnActivate")
    EndIf
EndFunction

Function EndTalkFallback()
    CancelTimer(68302)
    If B21TalkNPC != None && B21TalkNPC.GetReference() != None
        UnregisterForRemoteEvent(B21TalkNPC.GetReference(), "OnActivate")
    EndIf
    B21TalkNPC = None
    B21TalkScene = None
    B21TalkStage = -1
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If B21TalkStage < 0 || akActionRef != Game.GetPlayer() || IsStageDone(B21TalkStage)
        Return
    EndIf
    StartTimer(2.0, 68302)
EndEvent

Function TickTalkFallback()
    If B21TalkStage < 0 || !IsRunning()
        Return
    EndIf
    If IsStageDone(B21TalkStage)
        EndTalkFallback()
        Return
    EndIf
    ObjectReference speaker = None
    If B21TalkNPC != None
        speaker = B21TalkNPC.GetReference()
    EndIf
    If speaker != None && speaker.GetCurrentScene() != None
        Return
    EndIf
    If B21TalkScene != None && !B21TalkScene.IsPlaying()
        Scene kickoffScene = B21TalkScene
        B21TalkScene = None
        kickoffScene.Start()
        Return
    EndIf
    Int talkStage = B21TalkStage
    EndTalkFallback()
    SetStage(talkStage)
EndFunction

; Scenes without a completion stage (SCQS) advanced the FO76 event from the server.
; A missing, unloaded or silent scene advances immediately; a playing one is capped.
Function SetStageAfterScene(Scene akScene, Int aiStage, Float afMaxSeconds)
    If aiStage < 0 || IsStageDone(aiStage)
        Return
    EndIf
    B21PendingScene = akScene
    B21PendingSceneStage = aiStage
    B21PendingSceneSeconds = afMaxSeconds
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
    StartTimer(1.0, 68303)
EndFunction

Function TickPendingScene()
    If B21PendingSceneStage < 0 || !IsRunning()
        Return
    EndIf
    If IsStageDone(B21PendingSceneStage)
        B21PendingScene = None
        B21PendingSceneStage = -1
        Return
    EndIf
    B21PendingSceneSeconds -= 1.0
    If B21PendingScene == None || !B21PendingScene.IsPlaying() || B21PendingSceneSeconds <= 0.0
        Int pendingStage = B21PendingSceneStage
        B21PendingScene = None
        B21PendingSceneStage = -1
        SetStage(pendingStage)
        Return
    EndIf
    StartTimer(1.0, 68303)
EndFunction

Function ResetBombs()
    currentNumberOfPlacedBombs = 0.0
    currentBombAlias = None
    Int index = 0
    While bombAliases != None && index < bombAliases.GetCount()
        ObjectReference bomb = bombAliases.GetAt(index)
        If bomb != None
            bomb.DisableNoWait()
        EndIf
        index += 1
    EndWhile
EndFunction

Function BeginExplosivePhase()
    ArmSanctifiers()
    UpdatePlacedBombCount()
    StartTimer(1.0, 68301)
EndFunction

Function TickExplosivePhase()
    If !IsRunning() || !IsStageDone(200) || IsStageDone(stageToSetOnBombsPlaced)
        Return
    EndIf
    ArmSanctifiers()
    UpdatePlacedBombCount()
    If !IsStageDone(stageToSetOnBombsPlaced)
        StartTimer(1.0, 68301)
    EndIf
EndFunction

; Sanctifiers dropped Luca's explosives through a death item the FO4 NPC no longer
; has, so each living sanctifier carries one to its corpse instead.
Function ArmSanctifiers()
    If explosiveItem == None
        Return
    EndIf
    Int index = 0
    While sanctifierAliases != None && index < sanctifierAliases.GetCount()
        Actor sanctifier = sanctifierAliases.GetAt(index) as Actor
        If sanctifier != None && !sanctifier.IsDead() && sanctifier.GetItemCount(explosiveItem) == 0
            sanctifier.AddItem(explosiveItem, 1, True)
        EndIf
        index += 1
    EndWhile
EndFunction

Function UpdatePlacedBombCount()
    If bombAliases == None || !IsStageDone(200) || IsStageDone(stageToSetOnBombsPlaced)
        Return
    EndIf
    Int total = bombAliases.GetCount()
    Int placed = 0
    Int index = 0
    While index < total
        ObjectReference bomb = bombAliases.GetAt(index)
        If bomb != None && !bomb.IsDisabled()
            placed += 1
            currentBombAlias = bomb
        EndIf
        index += 1
    EndWhile
    currentNumberOfPlacedBombs = placed as Float
    SetQuestVariable("numberOfBombsPlaced", currentNumberOfPlacedBombs)
    SetQuestVariable("maxNumberOfBombsPlaced", total as Float)
    If total > 0 && placed >= total
        SetStage(stageToSetOnBombsPlaced)
    EndIf
EndFunction

Function DetonateBombs()
    If IsStageDone(stageToSetOnBombsDetonated)
        Return
    EndIf
    CancelTimer(68301)
    Int index = 0
    While bombAliases != None && index < bombAliases.GetCount()
        ObjectReference bomb = bombAliases.GetAt(index)
        If bomb != None && !bomb.IsDisabled()
            If explosionToPlace != None
                bomb.PlaceAtMe(explosionToPlace)
            EndIf
            bomb.DisableNoWait()
        EndIf
        index += 1
    EndWhile
    index = 0
    While doorAliases != None && index < doorAliases.GetCount()
        ObjectReference shackDoor = doorAliases.GetAt(index)
        If shackDoor != None
            shackDoor.DamageObject(bombDamage as Float)
        EndIf
        index += 1
    EndWhile
    If centerOfLocation != None && staticObjectToDestroy != None
        ObjectReference[] planks = centerOfLocation.FindAllReferencesOfType(staticObjectToDestroy, bombRadius as Float)
        index = 0
        While planks != None && index < planks.Length
            If planks[index] != None
                planks[index].DamageObject(bombDamage as Float)
            EndIf
            index += 1
        EndWhile
    EndIf
    currentNumberOfPlacedBombs = 0.0
    SetStage(stageToSetOnBombsDetonated)
EndFunction

Function RepairDamagedObjects()
    If centerOfLocation == None || objectsThatWillNeedRepair == None
        Return
    EndIf
    ObjectReference[] damaged = centerOfLocation.FindAllReferencesOfType(objectsThatWillNeedRepair, repairSearchRadius as Float)
    Int index = 0
    While damaged != None && index < damaged.Length
        If damaged[index] != None
            damaged[index].ClearDestruction()
        EndIf
        index += 1
    EndWhile
EndFunction

Function SpawnOgua()
    If spawnedOguaReference == None || oguaBase == None || spawnedOguaReference.GetReference() != None
        Return
    EndIf
    ObjectReference spawnPoint = centerOfLocation
    If oguaSpawnLoc != None && oguaSpawnLoc.GetReference() != None
        spawnPoint = oguaSpawnLoc.GetReference()
    EndIf
    If spawnPoint == None
        Return
    EndIf
    Int levelMod = oguaDifficulty
    If levelMod < 0 || levelMod > 4
        levelMod = 4
    EndIf
    Actor ogua = spawnPoint.PlaceActorAtMe(oguaBase, levelMod)
    If ogua == None
        Return
    EndIf
    spawnedOguaReference.ForceRefTo(ogua)
    If oguaSpawnFX != None
        spawnPoint.PlaceAtMe(oguaSpawnFX)
    EndIf
    If oguaSpawnSound != None
        oguaSpawnSound.Play(spawnPoint)
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        ogua.StartCombat(playerRef)
    EndIf
EndFunction

Function RemoveEventItemsFromPlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If cargoItem != None && playerRef.GetItemCount(cargoItem) > 0
        playerRef.RemoveItem(cargoItem, playerRef.GetItemCount(cargoItem), True)
    EndIf
    If explosiveItem != None && playerRef.GetItemCount(explosiveItem) > 0
        playerRef.RemoveItem(explosiveItem, playerRef.GetItemCount(explosiveItem), True)
    EndIf
EndFunction

Function CleanupEvent()
    CancelTimer(68301)
    CancelTimer(68303)
    B21PendingScene = None
    B21PendingSceneStage = -1
    EndTalkFallback()
    ResetBombs()
    RepairDamagedObjects()
    RemoveEventItemsFromPlayer()
    If spawnedOguaReference != None
        Actor ogua = spawnedOguaReference.GetActorReference()
        spawnedOguaReference.Clear()
        ; A killed Ogua stays as a lootable legendary corpse.
        If ogua != None && !ogua.IsDead()
            ogua.DisableNoWait()
            ogua.Delete()
        EndIf
    EndIf
EndFunction

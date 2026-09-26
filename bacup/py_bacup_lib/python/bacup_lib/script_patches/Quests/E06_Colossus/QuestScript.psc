Event OnQuestInit()
    ResetColossusEffects()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == FightStage
        BeginColossusFight()
    ElseIf auiStageID == StateOne || auiStageID == StateTwo || auiStageID == StateThree
        EscalateHazards()
    ElseIf auiStageID == 700
        EndColossusFight()
    ElseIf auiStageID == 900 || auiStageID == 9000 || auiStageID == 9990 || auiStageID == 9991 || auiStageID == 10000
        StopColossusEffects()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == eventBufferTimerID
        If FightActive()
            StartTimer(RandomSeconds(MinTimer, MaxTimer), hazardWaitTimerID)
            StartTimer(RandomSeconds(MinShakeTime, MaxShakeTime), cameraShakeTimerID)
        EndIf
    ElseIf aiTimerID == hazardWaitTimerID
        If FightActive()
            SpawnHazards()
            StartTimer(RandomSeconds(MinTimer, MaxTimer), hazardWaitTimerID)
        EndIf
    ElseIf aiTimerID == cameraShakeTimerID
        If FightActive()
            ShakePlayer(MTR08_CameraShakeSpell)
            randomShakeTime = RandomSeconds(MinShakeTime, MaxShakeTime)
            StartTimer(randomShakeTime, cameraShakeTimerID)
        EndIf
    ElseIf aiTimerID == durationTimerID
        If CollapseActive()
            ApplyCollapseEffects()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    StopColossusEffects()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If MTR08_ApplyCollapseFXSpell != None
            playerRef.DispelSpell(MTR08_ApplyCollapseFXSpell)
        EndIf
        If MTR08_CameraShakeSpellIntense != None
            playerRef.DispelSpell(MTR08_CameraShakeSpellIntense)
        EndIf
        If MTR08_CameraShakeSpell != None
            playerRef.DispelSpell(MTR08_CameraShakeSpell)
        EndIf
    EndIf
EndEvent

Float Function RandomSeconds(Float afMinimum, Float afMaximum)
    If afMinimum < 0.5
        afMinimum = 0.5
    EndIf
    If afMaximum < afMinimum
        afMaximum = afMinimum
    EndIf
    Return Utility.RandomFloat(afMinimum, afMaximum)
EndFunction

Function ResetColossusEffects()
    hasCombatStarted = False
    timerStarted = False
    numHazards = 1
    chosenHazard = None
    targetPlayer = None
    hazardScript = None
    PocketWatchScript = None
EndFunction

Bool Function FightActive()
    Return IsRunning() && hasCombatStarted && !IsStageDone(700) && !IsStageDone(900) && !IsStageDone(9990) && !IsStageDone(9991)
EndFunction

Bool Function CollapseActive()
    Return IsRunning() && IsStageDone(700) && !IsStageDone(900) && !IsStageDone(9000) && !IsStageDone(9990) && !IsStageDone(9991)
EndFunction

; The event is single-player here: the player is the only hazard target while inside the colossus arena.
Actor Function PlayerInArena()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || Activator_FallingRockHazards == None || Activator_FallingRockHazards.GetCount() == 0
        Return None
    EndIf
    ObjectReference anchor = Activator_FallingRockHazards.GetAt(0)
    If anchor == None || playerRef.GetParentCell() != anchor.GetParentCell()
        Return None
    EndIf
    Return playerRef
EndFunction

Actor Function PlayerInMine()
    Location mine = None
    If MineLocation != None
        mine = MineLocation.GetLocation()
    EndIf
    If mine == None
        Return PlayerInArena()
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || !playerRef.IsInLocation(mine)
        Return None
    EndIf
    Return playerRef
EndFunction

Function BeginColossusFight()
    BindPocketWatchColossus()
    If hasCombatStarted
        Return
    EndIf
    hasCombatStarted = True
    numHazards = 1
    timerStarted = True
    StartTimer(RandomSeconds(BufferTime, BufferTime), eventBufferTimerID)
EndFunction

Function EscalateHazards()
    BindPocketWatchColossus()
    Int desired = 1
    If IsStageDone(StateOne)
        desired += 1
    EndIf
    If IsStageDone(StateTwo)
        desired += 1
    EndIf
    If IsStageDone(StateThree)
        desired += 1
    EndIf
    If desired > numHazards
        numHazards = desired
    EndIf
    If !hasCombatStarted
        BeginColossusFight()
    EndIf
EndFunction

Int Function CountActiveHazards()
    Int active = 0
    Int index = 0
    While Activator_FallingRockHazards != None && index < Activator_FallingRockHazards.GetCount()
        Quests:E06_Colossus:HazardScript hazardRef = Activator_FallingRockHazards.GetAt(index) as Quests:E06_Colossus:HazardScript
        If hazardRef != None && hazardRef.IsHazardActive()
            active += 1
        EndIf
        index += 1
    EndWhile
    Return active
EndFunction

Function SpawnHazards()
    targetPlayer = PlayerInArena()
    If targetPlayer == None
        Return
    EndIf
    Int active = CountActiveHazards()
    While active < numHazards && ChooseHazard()
        hazardScript = chosenHazard as Quests:E06_Colossus:HazardScript
        hazardScript.StartHazard()
        active += 1
    EndWhile
    chosenHazard = None
    hazardScript = None
EndFunction

; Embers fall on the idle hazard closest to the player so the arena pressure follows the fight.
Bool Function ChooseHazard()
    chosenHazard = None
    Float bestDistance = 0.0
    Int index = 0
    While index < Activator_FallingRockHazards.GetCount()
        ObjectReference candidate = Activator_FallingRockHazards.GetAt(index)
        Quests:E06_Colossus:HazardScript hazardRef = candidate as Quests:E06_Colossus:HazardScript
        If hazardRef != None && !hazardRef.IsHazardActive()
            Float distance = candidate.GetDistance(targetPlayer)
            If chosenHazard == None || distance < bestDistance
                chosenHazard = candidate
                bestDistance = distance
            EndIf
        EndIf
        index += 1
    EndWhile
    Return chosenHazard != None
EndFunction

Function StopAllHazards()
    Int index = 0
    While Activator_FallingRockHazards != None && index < Activator_FallingRockHazards.GetCount()
        Quests:E06_Colossus:HazardScript hazardRef = Activator_FallingRockHazards.GetAt(index) as Quests:E06_Colossus:HazardScript
        If hazardRef != None
            hazardRef.StopHazard()
        EndIf
        index += 1
    EndWhile
EndFunction

Function ShakePlayer(Spell akShake)
    Actor playerRef = PlayerInArena()
    If playerRef != None && akShake != None
        akShake.Cast(playerRef, playerRef)
    EndIf
EndFunction

Function EndColossusFight()
    BindPocketWatchColossus()
    CancelTimer(eventBufferTimerID)
    CancelTimer(hazardWaitTimerID)
    CancelTimer(cameraShakeTimerID)
    timerStarted = False
    StopAllHazards()
    StartTimer(RandomSeconds(deathTime, deathTime), durationTimerID)
EndFunction

; The collapse effects last 15 seconds, so they are refreshed until the escape resolves.
Function ApplyCollapseEffects()
    If !CollapseActive()
        Return
    EndIf
    Actor playerRef = PlayerInMine()
    If playerRef != None
        If MTR08_CameraShakeSpellIntense != None
            MTR08_CameraShakeSpellIntense.Cast(playerRef, playerRef)
        EndIf
        If MTR08_ApplyCollapseFXSpell != None
            MTR08_ApplyCollapseFXSpell.Cast(playerRef, playerRef)
        EndIf
    EndIf
    StartTimer(14.0, durationTimerID)
EndFunction

Function StopColossusEffects()
    CancelTimer(eventBufferTimerID)
    CancelTimer(hazardWaitTimerID)
    CancelTimer(cameraShakeTimerID)
    CancelTimer(durationTimerID)
    StopAllHazards()
    hasCombatStarted = False
    timerStarted = False
    numHazards = 1
    chosenHazard = None
    targetPlayer = None
    hazardScript = None
EndFunction

; Something Sentimental puts Earle's pocket watch on the colossus through its own Colossus alias and stage 350.
Function BindPocketWatchColossus()
    If E06_PocketWatch == None || !E06_PocketWatch.IsRunning() || E06_PocketWatch.IsStageDone(350) || E06_PocketWatch.IsStageDone(400)
        Return
    EndIf
    If Alias_Wendigo_Colossus == None
        Return
    EndIf
    Actor colossus = Alias_Wendigo_Colossus.GetActorReference()
    If colossus == None
        Return
    EndIf
    PocketWatchScript = E06_PocketWatch as Quests:E06_Colossus:PocketWatch_QuestScript
    If PocketWatchScript == None || PocketWatchScript.Alias_Colossus == None
        Return
    EndIf
    If PocketWatchScript.Alias_Colossus.GetReference() != colossus
        PocketWatchScript.Alias_Colossus.ForceRefTo(colossus)
    EndIf
    If IsStageDone(700) || colossus.IsDead()
        E06_PocketWatch.SetStage(350)
    EndIf
EndFunction

Event OnQuestInit()
    pylonActivationLock = False
    numPylonsActivated = 0
    B21SecondWaveStarted = False
    B21EventFinishing = False
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 45401
        If !B21EventFinishing
            StartRobotWave("Mixed Robots")
        EndIf
    ElseIf aiTimerID == 45402
        CheckAssaultronHealth()
    ElseIf aiTimerID == 45403
        If IsRunning()
            Stop()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(45401)
    CancelTimer(45402)
    CancelTimer(45403)
    UnregisterPylons()
    ReleaseConduits()
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender == GetAssaultronActor() && !B21EventFinishing && !IsStageDone(stageIndex_Complete) && !IsStageDone(1000)
        SetStage(stageIndex_Complete)
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || pylonActivationLock || B21EventFinishing || !IsStageDone(250)
        Return
    EndIf
    Int pylonIndex = FindPylonIndex(akSender)
    If pylonIndex < 0
        Return
    EndIf
    PylonDatum pylon = PylonData[pylonIndex]
    If pylon.PylonPlayer == None || pylon.PylonPlayer.GetReference() != None
        Return
    EndIf
    pylonActivationLock = True
    pylon.PylonPlayer.ForceRefTo(playerRef)
    numPylonsActivated += 1
    SetObjectiveCompleted(pylon.PylonNotActivatedObjectiveIndex, True)
    SetObjectiveDisplayed(pylon.PylonActivatedObjectiveIndex, True, True)
    SetPylonModelActive(pylon, True)
    ApplyConduit(playerRef)
    UpdatePylonVariables()
    If numPylonsActivated >= PylonData.Length
        MakeAssaultronVulnerable()
    EndIf
    pylonActivationLock = False
EndEvent

Actor Function GetAssaultronActor()
    If Assaultron == None
        Return None
    EndIf
    Return Assaultron.GetActorRef()
EndFunction

DefaultQuestEncounterWaveScript Function GetEncounterWaves()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartRobotWave(String asIDString)
    DefaultQuestEncounterWaveScript ews = GetEncounterWaves()
    If ews != None && ews.FindEncounterWaveIndex(asIDString) >= 0
        ews.StartEncounterWaveByID(asIDString)
    EndIf
EndFunction

Function SetEventVariable(String asName, Float afValue)
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable(asName, afValue)
    EndIf
EndFunction

Function SetAssaultronShielded(Actor akAssaultron, Bool abShielded)
    If akAssaultron == None
        Return
    EndIf
    Quests:E01B_Encryptid:AssaultronBossScript bossScript = akAssaultron as Quests:E01B_Encryptid:AssaultronBossScript
    ; Ghosting stands in for the stealth field's damage immunity.
    akAssaultron.SetGhost(abShielded)
    If abShielded
        If AssaultronStealthSpell != None && !akAssaultron.HasSpell(AssaultronStealthSpell)
            akAssaultron.AddSpell(AssaultronStealthSpell, False)
        EndIf
        If bossScript != None
            bossScript.GoToState("invulnerable")
        EndIf
    Else
        If AssaultronStealthSpell != None
            akAssaultron.RemoveSpell(AssaultronStealthSpell)
        EndIf
        If bossScript != None
            bossScript.GoToState("vulnerable")
        EndIf
    EndIf
EndFunction

Function SpawnAssaultron()
    Actor assaultronActor = GetAssaultronActor()
    If assaultronActor != None
        If assaultronActor.IsDisabled()
            assaultronActor.Enable()
        EndIf
        If AssaultronDamagePerk != None && !assaultronActor.HasPerk(AssaultronDamagePerk)
            assaultronActor.AddPerk(AssaultronDamagePerk)
        EndIf
        SetAssaultronShielded(assaultronActor, numPylonsActivated < PylonData.Length)
        RegisterForRemoteEvent(assaultronActor, "OnDeath")
    EndIf
    Float firstWaveDelay = firstWaveTimer as Float
    If firstWaveDelay < 1.0
        firstWaveDelay = 1.0
    EndIf
    StartTimer(firstWaveDelay, 45401)
    StartTimer(2.0, 45402)
EndFunction

; The second robot wave arrives once the Assaultron has lost Wave2HealthPercentage of its health.
Function CheckAssaultronHealth()
    If B21EventFinishing || B21SecondWaveStarted
        Return
    EndIf
    Actor assaultronActor = GetAssaultronActor()
    If assaultronActor == None || assaultronActor.IsDead()
        Return
    EndIf
    If Health != None && assaultronActor.GetValuePercentage(Health) <= Wave2HealthPercentage
        B21SecondWaveStarted = True
        StartRobotWave("Gutsy Only")
        StartRobotWave("Eyebombs Only")
        Return
    EndIf
    StartTimer(2.0, 45402)
EndFunction

Int Function FindPylonIndex(ObjectReference akPylon)
    Int index = 0
    While akPylon != None && PylonData != None && index < PylonData.Length
        If PylonData[index] != None && PylonData[index].PylonAlias != None && PylonData[index].PylonAlias.GetReference() == akPylon
            Return index
        EndIf
        index += 1
    EndWhile
    Return -1
EndFunction

Function EnablePylons()
    Int index = 0
    While PylonData != None && index < PylonData.Length
        PylonDatum pylon = PylonData[index]
        If pylon != None
            If pylon.PylonAlias != None && pylon.PylonAlias.GetReference() != None
                RegisterForRemoteEvent(pylon.PylonAlias.GetReference(), "OnActivate")
            EndIf
            If pylon.PylonPlayer == None || pylon.PylonPlayer.GetReference() == None
                SetPylonModelActive(pylon, False)
                SetObjectiveDisplayed(pylon.PylonNotActivatedObjectiveIndex, True, True)
            EndIf
        EndIf
        index += 1
    EndWhile
    SetObjectiveDisplayed(objIndex_ActivatePylons, True, True)
    If E01B_Encryptid_Message_PylonsReady != None
        E01B_Encryptid_Message_PylonsReady.Show()
    EndIf
    UpdatePylonVariables()
EndFunction

Function UnregisterPylons()
    Int index = 0
    While PylonData != None && index < PylonData.Length
        If PylonData[index] != None && PylonData[index].PylonAlias != None && PylonData[index].PylonAlias.GetReference() != None
            UnregisterForRemoteEvent(PylonData[index].PylonAlias.GetReference(), "OnActivate")
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetPylonModelActive(PylonDatum akPylon, Bool abActive)
    If akPylon == None || akPylon.PylonModelAlias == None
        Return
    EndIf
    Quests:E01B_Encryptid:PylonScript pylonModel = akPylon.PylonModelAlias.GetReference() as Quests:E01B_Encryptid:PylonScript
    If pylonModel == None
        Return
    EndIf
    If abActive
        pylonModel.GoToState("active")
    Else
        pylonModel.GoToState("inactive")
    EndIf
EndFunction

Function UpdatePylonVariables()
    If PylonData != None
        SetEventVariable("Pylons_Total", PylonData.Length as Float)
    EndIf
    SetEventVariable("Pylons_Active", numPylonsActivated as Float)
EndFunction

; Single player: one conduit strength however many pylons the player holds. FO76 stacked the drain
; per pylon held, which a lone FO4 player holding all three could not survive.
Function ApplyConduit(Actor akPlayer)
    If akPlayer == None
        Return
    EndIf
    If E01B_Encryptid_PylonConduitSpellStrength != None
        akPlayer.SetValue(E01B_Encryptid_PylonConduitSpellStrength, PylonConduitSpellDamage)
    EndIf
    If E01B_Encryptid_PylonConduitSpell != None && !akPlayer.HasSpell(E01B_Encryptid_PylonConduitSpell)
        akPlayer.AddSpell(E01B_Encryptid_PylonConduitSpell, False)
    EndIf
    If PlayerConduitPerk != None && !akPlayer.HasPerk(PlayerConduitPerk)
        akPlayer.AddPerk(PlayerConduitPerk)
    EndIf
EndFunction

Function ReleaseConduits()
    Int index = 0
    While PylonData != None && index < PylonData.Length
        SetPylonModelActive(PylonData[index], False)
        index += 1
    EndWhile
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    If E01B_Encryptid_PylonConduitSpell != None
        playerRef.RemoveSpell(E01B_Encryptid_PylonConduitSpell)
    EndIf
    If PlayerConduitPerk != None && playerRef.HasPerk(PlayerConduitPerk)
        playerRef.RemovePerk(PlayerConduitPerk)
    EndIf
    If E01B_Encryptid_PylonConduitSpellStrength != None
        playerRef.SetValue(E01B_Encryptid_PylonConduitSpellStrength, 0.0)
    EndIf
EndFunction

Function MakeAssaultronVulnerable()
    Actor assaultronActor = GetAssaultronActor()
    If assaultronActor != None && !assaultronActor.IsDead()
        SetAssaultronShielded(assaultronActor, False)
        If AssaultronVulnerableExplosion != None
            assaultronActor.PlaceAtMe(AssaultronVulnerableExplosion)
        EndIf
    EndIf
    SetObjectiveCompleted(objIndex_ActivatePylons, True)
    SetObjectiveDisplayed(objIndex_DestroyRobot, True, True)
    If E01B_Encryptid_Message_Vulnerable != None
        E01B_Encryptid_Message_Vulnerable.Show()
    EndIf
EndFunction

Function ResolveOpenObjective(Int aiObjective, Bool abFailed)
    If aiObjective < 0 || !IsObjectiveDisplayed(aiObjective) || IsObjectiveCompleted(aiObjective) || IsObjectiveFailed(aiObjective)
        Return
    EndIf
    If abFailed
        SetObjectiveFailed(aiObjective, True)
    Else
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FinishEvent(Bool abFailed)
    If B21EventFinishing
        Return
    EndIf
    B21EventFinishing = True
    pylonActivationLock = True
    CancelTimer(45401)
    CancelTimer(45402)
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StopQuestTimer()
    EndIf
    ; FO76 stops spawning at the end; robots already fighting stay hostile until shutdown.
    DefaultQuestEncounterWaveScript ews = GetEncounterWaves()
    If ews != None
        ews.StopAllEncounterWaves(False)
    EndIf
    UnregisterPylons()
    ReleaseConduits()
    ResolveOpenObjective(5, abFailed)
    ResolveOpenObjective(10, abFailed)
    ResolveOpenObjective(20, abFailed)
    ResolveOpenObjective(objIndex_ActivatePylons, abFailed)
    ResolveOpenObjective(objIndex_DestroyRobot, abFailed)
    Int index = 0
    While PylonData != None && index < PylonData.Length
        If PylonData[index] != None
            ResolveOpenObjective(PylonData[index].PylonNotActivatedObjectiveIndex, abFailed)
            ResolveOpenObjective(PylonData[index].PylonActivatedObjectiveIndex, abFailed)
        EndIf
        index += 1
    EndWhile
    CancelTimer(45403)
    StartTimer(10.0, 45403)
EndFunction

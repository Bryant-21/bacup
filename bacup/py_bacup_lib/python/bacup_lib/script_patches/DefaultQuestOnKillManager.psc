Function SetVictimKeyword(Keyword NewVictimKeyword)
    VictimKeywordOverride = NewVictimKeyword
EndFunction

Function SetVictimFaction(Faction NewVictimFaction)
    VictimFactionOverride = NewVictimFaction
EndFunction

Function SetWeaponKeyword(Keyword NewWeaponKeyword)
    WeaponKeywordOverride = NewWeaponKeyword
EndFunction

Function SetVictimsRequired(Int NewVictimsRequiredAmount)
    VictimsRequiredOverride = NewVictimsRequiredAmount
EndFunction

Function StartTrackingKills()
    If !trackingKills
        CountVictims = 0
        trackingKills = True
        RegisterForRemoteEvent(Game.GetPlayer(), "OnKill")
    EndIf
EndFunction

Event OnQuestInit()
    If TrackKillsImmediately
        StartTrackingKills()
    EndIf
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
    If !trackingKills || incrementLock || akSender != Game.GetPlayer() || akVictim == None
        Return
    EndIf

    Keyword requiredVictimKeyword = VictimKeywordOverride
    If requiredVictimKeyword == None
        requiredVictimKeyword = VictimKeyword
    EndIf
    Race requiredVictimRace = VictimRaceOverride
    If requiredVictimRace == None
        requiredVictimRace = VictimRace
    EndIf
    Faction requiredVictimFaction = VictimFactionOverride
    If requiredVictimFaction == None
        requiredVictimFaction = VictimFaction
    EndIf
    Keyword requiredWeaponKeyword = WeaponKeywordOverride
    If requiredWeaponKeyword == None
        requiredWeaponKeyword = WeaponKeyword
    EndIf

    If requiredVictimKeyword != None && !akVictim.HasKeyword(requiredVictimKeyword)
        Return
    EndIf
    If requiredVictimRace != None && akVictim.GetRace() != requiredVictimRace
        Return
    EndIf
    If requiredVictimFaction != None && !akVictim.IsInFaction(requiredVictimFaction)
        Return
    EndIf
    If requiredWeaponKeyword != None
        Weapon equippedWeapon = akSender.GetEquippedWeapon()
        If equippedWeapon == None || !equippedWeapon.HasKeyword(requiredWeaponKeyword)
            Return
        EndIf
    EndIf

    incrementLock = True
    CountVictims += 1

    If VictimCountData != None
        Int datumIndex = 0
        While datumIndex < VictimCountData.Length
            If VictimCountData[datumIndex].CountOfVictims == CountVictims
                If VictimCountData[datumIndex].ObjectiveToComplete >= 0
                    SetObjectiveCompleted(VictimCountData[datumIndex].ObjectiveToComplete)
                EndIf
                If VictimCountData[datumIndex].ObjectiveToDisplay >= 0
                    SetObjectiveDisplayed(VictimCountData[datumIndex].ObjectiveToDisplay)
                EndIf
                If VictimCountData[datumIndex].StageToSet >= 0
                    SetStage(VictimCountData[datumIndex].StageToSet)
                EndIf
            EndIf
            datumIndex += 1
        EndWhile
    EndIf

    Int requiredVictimCount = VictimsRequired
    If VictimsRequiredOverride > 0
        requiredVictimCount = VictimsRequiredOverride
    EndIf
    If CountVictims >= requiredVictimCount
        If VictimCountObjectiveOverride >= 0
            SetObjectiveCompleted(VictimCountObjectiveOverride)
        ElseIf VictimCountObjective >= 0
            SetObjectiveCompleted(VictimCountObjective)
        EndIf
        If StageToSet >= 0
            SetStage(StageToSet)
        EndIf
        trackingKills = False
        UnregisterForRemoteEvent(Game.GetPlayer(), "OnKill")
    EndIf

    incrementLock = False
EndEvent

Event OnQuestShutdown()
    trackingKills = False
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnKill")
EndEvent

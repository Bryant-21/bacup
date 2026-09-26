Event OnAliasInit()
    B21LegendaryRolledRefs = new ObjectReference[0]
    RollForMembers()
    If doOnRefAdded
        StartTimer(1.0, 7642)
    EndIf
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    If doOnLoad
        RollForReference(akSenderRef, True)
    EndIf
EndEvent

; FO4 collections raise no event when a reference is added, so new members are noticed by polling.
Event OnTimer(Int aiTimerID)
    Quest owner = GetOwningQuest()
    If aiTimerID == 7642 && owner != None && owner.IsRunning()
        RollForMembers()
        StartTimer(1.0, 7642)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(7642)
    B21LegendaryRolledRefs = None
EndEvent

Function RollForMembers()
    If !doOnRefAdded && !doOnLoad
        Return
    EndIf
    Int index = GetCount() - 1
    While index >= 0
        RollForReference(GetAt(index), True)
        index -= 1
    EndWhile
EndFunction

Function RollForReference(ObjectReference akRef, Bool abAllowed)
    Actor target = akRef as Actor
    If !abAllowed || target == None || target.IsDead()
        Return
    EndIf
    If doOnLoad && !doOnRefAdded && !target.Is3DLoaded()
        Return
    EndIf
    If B21LegendaryRolledRefs == None
        B21LegendaryRolledRefs = new ObjectReference[0]
    EndIf
    If B21LegendaryRolledRefs.Find(target) >= 0
        Return
    EndIf
    ; Spawned wave actors that died or were deleted drop out of the collection; forget the oldest roll.
    If B21LegendaryRolledRefs.Length >= 120
        B21LegendaryRolledRefs.Remove(0)
    EndIf
    B21LegendaryRolledRefs.Add(target)
    If Utility.RandomInt(1, 100) > legendaryChance
        Return
    EndIf
    If !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return
    EndIf
    ActorValue rankValue = Game.GetFormFromFile(0x00FFD809, "B21_TalesFromAppalachia.esm") as ActorValue
    If rankValue == None || target.GetValue(rankValue) > 0.0
        Return
    EndIf
    Int lowRank = minRank
    If lowRank < 1
        lowRank = 1
    EndIf
    Int highRank = maxRank
    If highRank < lowRank
        highRank = lowRank
    EndIf
    If highRank > 5
        highRank = 5
    EndIf
    If lowRank > highRank
        lowRank = highRank
    EndIf
    target.SetValue(rankValue, Utility.RandomInt(lowRank, highRank) as Float)
EndFunction

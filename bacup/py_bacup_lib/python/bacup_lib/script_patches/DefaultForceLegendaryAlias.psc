Event OnAliasInit()
    B21LegendaryRolledRef = None
    RollForReference(GetReference(), doOnRefAdded || doOnLoad)
    If doOnRefAdded
        StartTimer(1.0, 7641)
    EndIf
EndEvent

Event OnLoad()
    If doOnLoad
        RollForReference(GetReference(), True)
    EndIf
EndEvent

; FO4 aliases raise no event when a script forces a new reference, so a fill is noticed by polling.
Event OnTimer(Int aiTimerID)
    Quest owner = GetOwningQuest()
    If aiTimerID == 7641 && owner != None && owner.IsRunning()
        RollForReference(GetReference(), True)
        StartTimer(1.0, 7641)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(7641)
    B21LegendaryRolledRef = None
EndEvent

Function RollForReference(ObjectReference akRef, Bool abAllowed)
    Actor target = akRef as Actor
    If !abAllowed || target == None || target == B21LegendaryRolledRef || target.IsDead()
        Return
    EndIf
    If doOnLoad && !doOnRefAdded && !target.Is3DLoaded()
        Return
    EndIf
    B21LegendaryRolledRef = target
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

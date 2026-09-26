Event OnQuestInit()
    ; The pack leaders are created references, so their aliases fill after quest
    ; init; FO4 raises no event for that and the ranks are applied by polling.
    StartTimer(1.0, 9537)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9537 || !IsRunning()
        Return
    EndIf
    If ApplyEpicRanks()
        Return
    EndIf
    StartTimer(2.0, 9537)
EndEvent

Event OnQuestShutdown()
    CancelTimer(9537)
EndEvent

ActorValue Function GetEpicRankValue()
    ; FO76's SQ_EpicCreatures manager has no FO4 equivalent; Tales carries the rank AV.
    If !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
        Return None
    EndIf
    Return Game.GetFormFromFile(0x00FFD809, "B21_TalesFromAppalachia.esm") as ActorValue
EndFunction

Bool Function ApplyEpicRank(ReferenceAlias akLeader, ActorValue akRankValue)
    If akLeader == None
        Return True
    EndIf
    Actor leaderRef = akLeader.GetActorReference()
    If leaderRef == None
        Return False
    EndIf
    If akRankValue != None && leaderRef.GetValue(akRankValue) <= 0.0 && !leaderRef.IsDead()
        ; Every wolf pack leader is a one-star legendary in FO76.
        leaderRef.SetValue(akRankValue, 1.0)
    EndIf
    Return True
EndFunction

Bool Function ApplyEpicRanks()
    ActorValue rankValue = GetEpicRankValue()
    Bool allFilled = ApplyEpicRank(Pack1Leader, rankValue)
    If !ApplyEpicRank(Pack2Leader, rankValue)
        allFilled = False
    EndIf
    If !ApplyEpicRank(Pack3Leader, rankValue)
        allFilled = False
    EndIf
    Return allFilled
EndFunction

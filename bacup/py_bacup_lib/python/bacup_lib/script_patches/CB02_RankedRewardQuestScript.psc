Function CollectRankedPlayers()
    RankedPlayers = New Actor[0]
    Actor playerRef = Game.GetPlayer()
    ; FO76 ranked every participant; single player has exactly one contender.
    If playerRef != None && RankedActorValue != None && playerRef.GetValue(RankedActorValue) > 0.0
        RankedPlayers.Add(playerRef)
    EndIf
EndFunction

Function PublishRankVariable(String asVariableName, Float afValue)
    If asVariableName == ""
        Return
    EndIf
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables != None
        variables.SetVariable(asVariableName, afValue)
    EndIf
    ; The companion mask-team quest shows the same standings in its own objectives.
    B21:QuestVariables teamVariables = CB02_MaskTeam as B21:QuestVariables
    If teamVariables != None
        teamVariables.SetVariable(asVariableName, afValue)
    EndIf
EndFunction

Function ApplyRankedRewardRow(Int aiRow, Actor akRankedPlayer)
    RankedRewardAliasDatum rankRow = RankedRewardAliasData[aiRow]
    If akRankedPlayer == None
        Return
    EndIf
    If rankRow.RefCol != None && rankRow.RefCol.Find(akRankedPlayer) < 0
        rankRow.RefCol.AddRef(akRankedPlayer)
    EndIf
    If rankRow.SingleRefAliasToForceInto != None
        rankRow.SingleRefAliasToForceInto.ForceRefTo(akRankedPlayer)
    EndIf
    Float rankedValue = 0.0
    If RankedActorValue != None
        rankedValue = akRankedPlayer.GetValue(RankedActorValue)
    EndIf
    PublishRankVariable(rankRow.TxtVarToUpdate, rankedValue)
    If rankRow.ObjectiveToDisplay > 0
        SetObjectiveDisplayed(rankRow.ObjectiveToDisplay, True)
    EndIf
EndFunction

Function UpdateRankedPlayers()
    If UpdateRankedPlayersSpinLock || RankedRewardAliasData == None || RankedActorValue == None
        Return
    EndIf
    UpdateRankedPlayersSpinLock = True
    CollectRankedPlayers()

    Int nextPlayer = 0
    Int row = 0
    While row < RankedRewardAliasData.Length
        RankedRewardAliasDatum rankRow = RankedRewardAliasData[row]
        Int placesInRow = rankRow.LimitBeforeNextRank
        If placesInRow < 1
            placesInRow = 1
        EndIf
        Int placed = 0
        While placed < placesInRow && nextPlayer < RankedPlayers.Length
            Actor candidate = RankedPlayers[nextPlayer]
            If candidate != None && candidate.GetValue(RankedActorValue) >= rankRow.MinActorValueRequired
                ApplyRankedRewardRow(row, candidate)
                nextPlayer += 1
            Else
                placed = placesInRow
            EndIf
            placed += 1
        EndWhile
        row += 1
    EndWhile
    UpdateRankedPlayersSpinLock = False
EndFunction

Function ClearRankedPlayers()
    RankedPlayers = New Actor[0]
    Int row = 0
    While RankedRewardAliasData != None && row < RankedRewardAliasData.Length
        RankedRewardAliasDatum rankRow = RankedRewardAliasData[row]
        If rankRow.RefCol != None
            rankRow.RefCol.RemoveAll()
        EndIf
        If rankRow.SingleRefAliasToForceInto != None
            rankRow.SingleRefAliasToForceInto.Clear()
        EndIf
        PublishRankVariable(rankRow.TxtVarToUpdate, 0.0)
        row += 1
    EndWhile
EndFunction

Event OnQuestInit()
    UpdateRankedPlayersSpinLock = False
    ; The ranking actor value carries over between runs, so the tally starts clean.
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && RankedActorValue != None
        playerRef.SetValue(RankedActorValue, 0.0)
    EndIf
    ClearRankedPlayers()
EndEvent

Event OnQuestShutdown()
    ClearRankedPlayers()
EndEvent

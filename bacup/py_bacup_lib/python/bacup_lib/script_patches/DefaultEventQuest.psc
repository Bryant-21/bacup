Event OnQuestInit()
    B21PlayerJoinedEvent = False
    B21PlayerParticipating = False
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    RefreshParticipation()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RefreshParticipation()
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshParticipation()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7601
        RefreshParticipation()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7601)
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent

Bool Function IsPlayerParticipating()
    If AliasEventPlayers == None
        Return True
    EndIf
    Return B21PlayerParticipating
EndFunction

Bool Function RefWithinAliasRadius(ObjectReference akSubject, ReferenceAlias akAlias, Float afRadius)
    If akSubject == None || akAlias == None || afRadius <= 0.0
        Return False
    EndIf
    ObjectReference anchor = akAlias.GetReference()
    If anchor == None
        Return False
    EndIf
    If akSubject.GetWorldSpace() != anchor.GetWorldSpace()
        Return False
    EndIf
    If akSubject.GetWorldSpace() == None && akSubject.GetParentCell() != anchor.GetParentCell()
        Return False
    EndIf
    Return akSubject.GetDistance(anchor) <= afRadius
EndFunction

Bool Function PlayerInsideEventArea(Actor akPlayer, Bool abExitRadius)
    Float radius = ActivityEnterRadius
    If abExitRadius
        ; FO76 documents an exit radius of 1 as "leave when exiting the interior"; never let exit be tighter than entry.
        radius = ActivityExitRadius
        If radius < ActivityEnterRadius
            radius = ActivityEnterRadius
        EndIf
        If PlayerInsideCenterLocation(akPlayer)
            Return True
        EndIf
    EndIf
    If RefWithinAliasRadius(akPlayer, CenterMarker, radius)
        Return True
    EndIf

    If PlayerInsideExtraAreas(akPlayer, ExtraEntranceRadius)
        Return True
    EndIf
    Return abExitRadius && StartWithExtraExitRadiusEnabled && PlayerInsideExtraAreas(akPlayer, ExtraExitRadius)
EndFunction

Bool Function PlayerInsideCenterLocation(Actor akPlayer)
    If CenterMarker == None || CenterMarker.GetReference() == None
        Return False
    EndIf
    Location centerLocation = CenterMarker.GetReference().GetCurrentLocation()
    Location playerLocation = akPlayer.GetCurrentLocation()
    If centerLocation == None || playerLocation == None
        Return False
    EndIf
    Return playerLocation == centerLocation || playerLocation.IsChild(centerLocation)
EndFunction

Bool Function PlayerInsideExtraAreas(Actor akPlayer, EntranceRadiusData[] akAreas)
    Int index = 0
    While akAreas != None && index < akAreas.Length
        EntranceRadiusData area = akAreas[index]
        If area != None && RefWithinAliasRadius(akPlayer, area.RefAlias, area.EntranceRadius)
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function RefreshParticipation()
    If IsStopping() || IsStopped() || IsCompleted()
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || AliasEventPlayers == None
        Return
    EndIf

    Bool member = AliasEventPlayers.Find(playerRef) >= 0
    If member
        If !PlayerInsideEventArea(playerRef, True)
            AliasEventPlayers.RemoveRef(playerRef)
            member = False
        EndIf
    ElseIf PlayerInsideEventArea(playerRef, False)
        Bool lateJoinClosed = EventStageDisableLateJoin >= 0 && IsStageDone(EventStageDisableLateJoin)
        If !lateJoinClosed || B21PlayerJoinedEvent
            AliasEventPlayers.AddRef(playerRef)
            B21PlayerJoinedEvent = True
            member = True
        EndIf
    EndIf
    B21PlayerParticipating = member

    If IsRunning() || IsStarting()
        StartTimer(2.0, 7601)
    EndIf
EndFunction

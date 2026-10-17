; RE_SceneKMK13 / RE_TravelKMK12: the encounter's map marker is revealed once the player comes within NearbyDistance.
Event OnQuestInit()
    StartTimer(2.0, 41)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 41 || !IsRunning()
        Return
    EndIf
    ObjectReference marker = Alias_MapMarker.GetReference()
    If marker == None
        Return
    EndIf
    If Game.GetPlayer().GetDistance(marker) <= NearbyDistance
        marker.AddToMap()
        Return
    EndIf
    StartTimer(2.0, 41)
EndEvent

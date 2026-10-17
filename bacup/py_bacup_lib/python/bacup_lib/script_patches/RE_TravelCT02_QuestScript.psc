; RE_TravelCT02 Haven Church eyebot. Once the player reaches the advertising eyebot, Haven Church's map marker is
; revealed.
Event OnQuestInit()
    StartTimer(2.0, 1)
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning() || HavenChurchMapMarkerRef == None
        Return
    EndIf
    ObjectReference eyebotRef = Eyebot.GetReference()
    If eyebotRef != None && Game.GetPlayer().GetDistance(eyebotRef) <= 1000.0
        HavenChurchMapMarkerRef.AddToMap()
        Return
    EndIf
    StartTimer(2.0, 1)
EndEvent

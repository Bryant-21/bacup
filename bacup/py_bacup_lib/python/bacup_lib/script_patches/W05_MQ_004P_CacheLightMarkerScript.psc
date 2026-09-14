Event OnLoad()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || TrackingValue == None || playerRef.GetValue(TrackingValue) < 1.0
        Return
    EndIf

    ObjectReference nextMarker = GetLinkedRef()
    If nextMarker == None || !nextMarker.IsDisabled()
        Return
    EndIf

    If EnableSound
        EnableSound.Play(Self)
    EndIf
    If EnableDelay > 0.0
        Utility.Wait(EnableDelay)
    EndIf
    nextMarker.Enable()
EndEvent

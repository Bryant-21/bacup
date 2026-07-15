Event OnActivate(ObjectReference akActionRef)
    If SoundToPlay != None
        SoundToPlay.Play(Self)
    EndIf
EndEvent

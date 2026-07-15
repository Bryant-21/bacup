Function ResolveKlaxonSound()
    If KlaxonSound == None && LinkKlaxonSound != None
        KlaxonSound = GetLinkedRef(LinkKlaxonSound)
    EndIf
EndFunction

Function UpdateKlaxonSound(Bool playSound)
    ResolveKlaxonSound()
    If KlaxonSound == None
        Return
    EndIf

    If playSound && ShouldPlayKlaxonSound
        KlaxonSound.Enable()
    Else
        KlaxonSound.Disable()
    EndIf
EndFunction

Event OnLoad()
    Parent.OnLoad()
    UpdateKlaxonSound(IsOpen)
EndEvent

State closed
    Event OnBeginState(String asOldState)
        Parent.OnBeginState(asOldState)
        UpdateKlaxonSound(False)
    EndEvent
EndState

State open
    Event OnBeginState(String asOldState)
        Parent.OnBeginState(asOldState)
        UpdateKlaxonSound(True)
    EndEvent
EndState

; `default` is a reserved Papyrus keyword. Rename the source state and its exact
; GoToState target while retaining the source state's members and Auto flag.
; @state-rename default operational

; Compatibility fills for the transmitter's public operations.

Function DoDestructionFX()
    If BoS_Transmitter_Explosion != None
        PlaceAtMe(BoS_Transmitter_Explosion)
    EndIf
    If Is3DLoaded()
        PlayAnimation("Play01")
        PlayAnimation("Jumpstate02")
    EndIf
EndFunction

Function DestroyTransmitter()
    If !predictivelyBroken
        predictivelyBroken = True
        DoDestructionFX()
    EndIf
EndFunction

Function RestoreToDefault()
    predictivelyBroken = False
    GoToState("operational")
EndFunction

Event OnReset()
    RestoreToDefault()
EndEvent

State broken
    Event OnBeginState(String asOldState)
        DestroyTransmitter()
    EndEvent

    Event OnLoad()
        PlayAnimation("Play01")
        PlayAnimation("Jumpstate02")
    EndEvent
EndState

Function UpdateLocalPowerState()
    Bool hasPower = pBoS01 != None && pBoS01.GetStage() >= 400
    ToggleButtonScript toggle = (Self as ObjectReference) as ToggleButtonScript
    If toggle != None
        toggle.SetActiveNoWait(hasPower)
    Else
        BlockActivation(!hasPower, False)
    EndIf
EndFunction

Event OnInit()
    UpdateLocalPowerState()
EndEvent

Event OnLoad()
    UpdateLocalPowerState()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    UpdateLocalPowerState()
    If pBoS01 == None || pBoS01.GetStage() < 400
        If pLC004_ToggleButtonInactiveMessage != None
            pLC004_ToggleButtonInactiveMessage.Show()
        EndIf
        Return
    EndIf
    ; The co-attached FO4 ToggleButtonScript activates LinkedRefToActivate (TwoStateCollisionKeyword),
    ; but this placed button links the Fort Defiance gate without a keyword.
    Default2StateActivator gate = GetLinkedRef() as Default2StateActivator
    If gate != None && !gate.isOpen
        gate.SetOpenNoWait(True)
    EndIf
EndEvent

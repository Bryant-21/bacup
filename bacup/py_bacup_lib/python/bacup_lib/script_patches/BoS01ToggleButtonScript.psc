Function UpdateLocalPowerState()
    Bool hasPower = pBoS01 != None && pBoS01.GetStage() >= 400
    BlockActivation(!hasPower, False)
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
    EndIf
EndEvent

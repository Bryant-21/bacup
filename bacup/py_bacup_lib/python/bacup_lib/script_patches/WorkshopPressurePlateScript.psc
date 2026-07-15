; Method fill for the hollow FO76 WorkshopPressurePlateScript. The decompiled
; skeleton supplies Count, PlateLowered, their backing fields, and the bound
; up/down sounds. Keep a count because trigger enter/leave events may overlap.

Event OnTriggerEnter(ObjectReference akActionRef)
    CheckCount()
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    CheckCount()
EndEvent

Function CountSet(Int NewCount)
    If NewCount < 0
        NewCount = 0
    EndIf

    If CountActual != NewCount
        CountActual = NewCount
        CheckCount()
    EndIf
EndFunction

Function CheckCount()
    Int NewCount = GetTriggerObjectCount()
    If NewCount < 0
        NewCount = 0
    EndIf

    CountActual = NewCount
    PlateLoweredSet(CountActual > 0)
EndFunction

Function PlateLoweredSet(Bool SetLowered)
    If PlateLoweredActual == SetLowered
        Return
    EndIf

    PlateLoweredActual = SetLowered
    If SetLowered
        PlayAnimation("On")
        If TRPWorkshopPressurePlateDown != None
            TRPWorkshopPressurePlateDown.Play(Self)
        EndIf
    Else
        PlayAnimation("Off")
        If TRPWorkshopPressurePlateUp != None
            TRPWorkshopPressurePlateUp.Play(Self)
        EndIf
    EndIf

    If (SetLowered && TransmitPowerOnPress) || (!SetLowered && !TransmitPowerOnPress)
        Activate(Self, True)
    EndIf
EndFunction

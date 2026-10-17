; Each menu entry shows one screen image (a linked ref) and hides the others.
Function ShowOnlyScreen(ObjectReference akTerminalRef, Keyword akShow)
    Keyword[] screens = new Keyword[4]
    screens[0] = LinkCustom01
    screens[1] = LinkCustom02
    screens[2] = LinkCustom03
    screens[3] = LinkCustom04
    Int i = 0
    While i < screens.Length
        If screens[i] && screens[i] != akShow
            ObjectReference other = akTerminalRef.GetLinkedRef(screens[i])
            If other
                other.Disable()
            EndIf
        EndIf
        i += 1
    EndWhile
    ObjectReference shown = akTerminalRef.GetLinkedRef(akShow)
    If shown
        shown.Enable()
    EndIf
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    ShowOnlyScreen(akTerminalRef, LinkCustom01)
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
    ShowOnlyScreen(akTerminalRef, LinkCustom04)
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    ShowOnlyScreen(akTerminalRef, LinkCustom02)
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    ShowOnlyScreen(akTerminalRef, LinkCustom03)
EndFunction

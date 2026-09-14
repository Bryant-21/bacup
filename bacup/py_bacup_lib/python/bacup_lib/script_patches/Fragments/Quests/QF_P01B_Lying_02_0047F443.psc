Function Fragment_Stage_0550_Item_00()
    ObjectReference noteRef = Alias_note_BoPeepNote.GetReference()
    If noteRef != None
        noteRef.Enable(False)
    EndIf
    ObjectReference keycardRef = Alias_key_Keycard.GetReference()
    If keycardRef != None
        keycardRef.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0950_Item_00()
    ObjectReference passwordRef = Alias_key_TerminalPassword.GetReference()
    If passwordRef != None
        passwordRef.Enable(False)
    EndIf
EndFunction

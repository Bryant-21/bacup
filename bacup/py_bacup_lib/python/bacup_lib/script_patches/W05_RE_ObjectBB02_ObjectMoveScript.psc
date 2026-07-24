Event OnInit()
    ObjectReference kTable = Table.GetReference()
    If kTable == None
        Return
    EndIf
    ObjectReference kNote = Note.GetReference()
    If kNote != None
        kNote.MoveTo(kTable)
    EndIf
    ObjectReference kBox = Box.GetReference()
    If kBox != None
        kBox.MoveTo(kTable)
    EndIf
EndEvent

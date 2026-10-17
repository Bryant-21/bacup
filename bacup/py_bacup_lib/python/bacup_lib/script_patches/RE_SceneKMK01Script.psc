; RE_SceneKMK01 / RE_ObjectMP11: the note names a dungeon; reading it off the body also reveals that dungeon's
; map marker (Alias_DungeonMapMarker, filled from the DUNGEON location's map-marker ref type).
Function NotePickedUp()
    Parent.NotePickedUp()
    ObjectReference marker = Alias_DungeonMapMarker.GetReference()
    If marker != None
        marker.AddToMap()
    EndIf
EndFunction
